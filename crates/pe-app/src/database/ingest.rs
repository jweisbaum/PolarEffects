//! Idempotent, transaction-per-race ingestion. Provider identities are scoped to races.
use super::{DatabaseSettings, check, connect};
use crate::error::{AppError, Context, Result};
use pe_trackers::TrackerEvent;
use postgres::{GenericClient, Transaction};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path, sync::atomic::AtomicBool};

fn guid(key: &str) -> String {
    uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_URL, key.as_bytes()).to_string()
}
/// Only this module supplies table/column names. Values always use bind parameters.
fn upsert(tx: &mut Transaction<'_>, table: &str, mut value: Value) -> Result<()> {
    let object = value
        .as_object_mut()
        .ok_or_else(|| AppError::Internal("Expected database record".into()))?;
    object.retain(|_, v| !v.is_null());
    let keys: Vec<_> = object
        .keys()
        .map(|k| format!("\"{}\"", k.replace('"', "\"\"")))
        .collect();
    let updates: Vec<_> = keys
        .iter()
        .filter(|k| k.as_str() != "\"id\"" && k.as_str() != "\"createdAt\"")
        .map(|k| format!("{k}=EXCLUDED.{k}"))
        .collect();
    let action = if updates.is_empty() {
        "DO NOTHING".to_owned()
    } else {
        format!("DO UPDATE SET {}", updates.join(","))
    };
    tx.execute(&format!("INSERT INTO public.\"{table}\" ({}) SELECT {} FROM json_populate_record(NULL::public.\"{table}\",$1) ON CONFLICT (id) {action}",keys.join(","),keys.join(",")),&[&value]).doing("save scraper row in",table)?;
    Ok(())
}
fn existing<C: GenericClient>(
    client: &mut C,
    source: &str,
    event: &pe_trackers::EventRef,
) -> Result<Option<Value>> {
    let rows=client.query(r#"SELECT row_to_json(c) FROM public."CompetitionUnits" c JOIN public."CalendarEvents" e ON e.id=c."calendarEventId" WHERE e.source=$1"#,&[&source]).doing("find existing","race")?;
    for row in rows {
        let c: Value = row.get(0);
        let key = c["scrapedOriginalId"].as_str().unwrap_or_default();
        let url = c["scrapedUrl"].as_str().unwrap_or_default();
        if same_key(source, key, &event.key)
            || pe_trackers::library::resolve_source(source, url)
                .is_ok_and(|r| r.tracker == event.tracker && same_key(source, &r.key, &event.key))
        {
            return Ok(Some(c));
        }
    }
    Ok(None)
}
fn same_key(source: &str, left: &str, right: &str) -> bool {
    if source == "YELLOWBRICK" {
        left.eq_ignore_ascii_case(right)
    } else {
        left == right
    }
}
fn family(key: &str) -> &str {
    key.split('?').next().unwrap_or(key)
}
type Participant = (Value, Value, bool);
fn participant<'a>(candidates: &'a [Participant], name: &str) -> Result<Option<&'a Participant>> {
    // Historical groups can contain several vessels with the same provider ID.
    // Prefer the participant actually referenced by this race's track, then its
    // name. Ambiguity must not silently overwrite a different boat's file.
    for choices in [
        candidates
            .iter()
            .filter(|(_, v, track)| *track && v["publicName"] == name)
            .collect::<Vec<_>>(),
        candidates.iter().filter(|(_, _, track)| *track).collect(),
        candidates
            .iter()
            .filter(|(_, v, _)| v["publicName"] == name)
            .collect(),
        candidates.iter().collect(),
    ] {
        if choices.len() == 1 {
            return Ok(choices.first().copied());
        }
    }
    if candidates.is_empty() {
        Ok(None)
    } else {
        Err(AppError::Internal(format!(
            "Ambiguous existing participants for {name}; race was not committed"
        )))
    }
}
/// A newly discovered Geovoile leg must reuse the calendar event of older
/// legs, even when that event was created upstream with a random GUID.
fn calendar(
    tx: &mut Transaction<'_>,
    source: &str,
    event: &pe_trackers::EventRef,
    catalogue: Option<&pe_trackers::library::yellowbrick::Race>,
) -> Result<Option<Value>> {
    let rows = tx.query(r#"SELECT row_to_json(e),c."scrapedUrl" FROM public."CalendarEvents" e LEFT JOIN public."CompetitionUnits" c ON c."calendarEventId"=e.id WHERE e.source=$1 ORDER BY e."createdAt",e.id"#, &[&source]).doing("match", "calendar event")?;
    for row in rows {
        let e: Value = row.get(0);
        let unit_url: Option<String> = row.get(1);
        if catalogue.is_some_and(|r| e["scrapedOriginalId"].as_str() == Some(r.id.as_str()))
            || e["scrapedOriginalId"]
                .as_str()
                .is_some_and(|s| same_key(source, family(s), family(&event.key)))
            || [unit_url.as_deref(), e["externalUrl"].as_str()]
                .into_iter()
                .flatten()
                .any(|url| {
                    pe_trackers::library::resolve_source(source, url)
                        .is_ok_and(|old| same_key(source, family(&old.key), family(&event.key)))
                })
        {
            return Ok(Some(e));
        }
    }
    Ok(None)
}
pub(super) fn known_urls(settings: &DatabaseSettings) -> Result<Vec<String>> {
    let mut c = connect(settings)?;
    Ok(c.query(r#"SELECT c."scrapedUrl",e.source FROM public."CompetitionUnits" c JOIN public."CalendarEvents" e ON e.id=c."calendarEventId" WHERE e.source IN ('YELLOWBRICK','GEOVOILE','BLUEWATER') AND c."scrapedUrl" IS NOT NULL ORDER BY c."startTime" DESC NULLS LAST"#,&[]).doing("list","known race URLs")?.iter().filter_map(|r|{let u:String=r.get(0);let source:String=r.get(1);pe_trackers::library::resolve_source(&source,&u).ok().map(|e|e.url)}).collect())
}
pub(super) fn yellowbrick_codes(
    settings: &DatabaseSettings,
) -> Result<BTreeMap<String, Vec<String>>> {
    let mut client = connect(settings)?;
    let rows = client.query(r#"SELECT e."scrapedOriginalId",c."scrapedUrl" FROM public."CalendarEvents" e JOIN public."CompetitionUnits" c ON c."calendarEventId"=e.id WHERE e.source='YELLOWBRICK' AND e."scrapedOriginalId" IS NOT NULL AND c."scrapedUrl" IS NOT NULL ORDER BY e.id,c.id"#, &[]).doing("read", "YellowBrick race codes")?;
    let mut codes: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for row in rows {
        codes.entry(row.get(0)).or_default().push(row.get(1));
    }
    Ok(codes)
}
/// Historical completed races with all registered files present need no network calls.
pub(super) fn complete(settings: &DatabaseSettings, event: &pe_trackers::EventRef) -> Result<bool> {
    let mut client = connect(settings)?;
    let Some(c) = existing(
        &mut client,
        pe_trackers::library::source(event.tracker),
        event,
    )?
    else {
        return Ok(false);
    };
    if c["isCompleted"] != true {
        return Ok(false);
    }
    let id = c["id"].as_str().unwrap_or_default();
    let rows=client.query(r#"SELECT "providedStorageKey" FROM public."VesselParticipantTrackJsons" WHERE "competitionUnitId"=$1::text::uuid"#,&[&id]).doing("check","saved track files")?;
    Ok(!rows.is_empty()
        && rows.iter().all(|r| {
            let key: String = r.get(0);
            safe_file(Path::new(&settings.geojson_directory), &key).is_ok_and(|p| p.is_file())
        }))
}
fn safe_file(root: &Path, key: &str) -> Result<std::path::PathBuf> {
    if Path::new(key)
        .components()
        .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return Err(AppError::Internal(
            "Unsafe GeoJSON storage key in database".into(),
        ));
    }
    let path = root.join(key);
    let canonical_root = root.canonicalize().doing("read", root.display())?;
    let mut ancestor = path.as_path();
    while !ancestor.exists() {
        ancestor = ancestor
            .parent()
            .ok_or_else(|| AppError::Internal("Invalid track path".into()))?;
    }
    if !ancestor
        .canonicalize()
        .doing("read", ancestor.display())?
        .starts_with(&canonical_root)
    {
        return Err(AppError::Internal(
            "Track storage key points outside the GeoJSON directory".into(),
        ));
    }
    Ok(path)
}

pub(super) fn save(
    settings: &DatabaseSettings,
    event: &TrackerEvent,
    original_url: &str,
    geometries: &[Value],
    catalogue: Option<&pe_trackers::library::yellowbrick::Race>,
    cancel: &AtomicBool,
) -> Result<usize> {
    if settings.geojson_directory.is_empty() {
        return Err(AppError::Internal(
            "Choose the GeoJSON directory in Settings".into(),
        ));
    }
    let root = Path::new(&settings.geojson_directory);
    std::fs::create_dir_all(root).doing("create", root.display())?;
    let mut client = connect(settings)?;
    let mut tx = client.transaction().doing("begin", "race transaction")?;
    // Serializes our writers, including two app instances, before reading old identities.
    tx.query_one("SELECT pg_advisory_xact_lock(7284173091)", &[])
        .doing("lock", "race ingestion")?;
    let now: String = tx
        .query_one("SELECT now()::text", &[])
        .doing("read", "database time")?
        .get(0);
    let source = pe_trackers::library::source(event.event.tracker);
    let key = format!("polareffects/syrf/{source}/{}", event.event.key);
    let prior = existing(&mut tx, source, &event.event)?;
    let id_for = |field: &str, suffix: &str| {
        prior
            .as_ref()
            .and_then(|c| c[field].as_str())
            .map(str::to_owned)
            .unwrap_or_else(|| guid(&format!("{key}/{suffix}")))
    };
    let cid = id_for("id", "race");
    let catalogue =
        catalogue.filter(|_| event.event.tracker == pe_core::track::Tracker::YellowBrick);
    let event_key = catalogue.map_or_else(|| family(&event.event.key), |r| r.id.as_str());
    let old_calendar = if let Some(eid) = prior.as_ref().and_then(|c| c["calendarEventId"].as_str())
    {
        tx.query_opt(
            r#"SELECT row_to_json(e) FROM public."CalendarEvents" e WHERE id=$1::text::uuid"#,
            &[&eid],
        )
        .doing("read", "calendar event")?
        .map(|r| r.get::<_, Value>(0))
    } else {
        calendar(&mut tx, source, &event.event, catalogue)?
    };
    let eid = prior
        .as_ref()
        .and_then(|c| c["calendarEventId"].as_str())
        .or_else(|| old_calendar.as_ref().and_then(|e| e["id"].as_str()))
        .map(str::to_owned)
        .unwrap_or_else(|| guid(&format!("polareffects/syrf/{source}/{event_key}/event")));
    let original_url = prior
        .as_ref()
        .and_then(|c| c["scrapedUrl"].as_str())
        .filter(|u| !u.is_empty())
        .unwrap_or(original_url);
    let group = id_for("vesselParticipantGroupId", "group");
    let course = id_for("courseId", "course");
    let stamp = |t: Option<i64>| {
        t.and_then(|t| chrono::DateTime::from_timestamp(t, 0).map(|d| d.to_rfc3339()))
    };
    let start = stamp(event.start);
    let end = stamp(event.stop);
    let calendar_url = old_calendar
        .as_ref()
        .and_then(|e| e["externalUrl"].as_str())
        .filter(|u| !u.is_empty())
        .unwrap_or(original_url);
    let calendar_name = old_calendar
        .as_ref()
        .and_then(|e| e["name"].as_str())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| catalogue.map_or(event.title.as_str(), |r| r.title.as_str()));
    let calendar_time = |field: &str, time: Option<i64>, earliest: bool| {
        let old = old_calendar
            .as_ref()
            .and_then(|e| e[field].as_str())
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|t| t.timestamp());
        stamp(match (old, time) {
            (Some(a), Some(b)) => Some(if earliest { a.min(b) } else { a.max(b) }),
            (a, b) => a.or(b),
        })
    };
    let calendar_start = calendar_time("approximateStartTime", event.start, true);
    let calendar_end = calendar_time("approximateEndTime", event.stop, false);
    let calendar_original_id = old_calendar
        .as_ref()
        .and_then(|e| e["scrapedOriginalId"].as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(event_key);
    let unit_original_id = prior
        .as_ref()
        .and_then(|e| e["scrapedOriginalId"].as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(&event.event.key);
    let private = old_calendar
        .as_ref()
        .and_then(|e| e["isPrivate"].as_bool())
        .unwrap_or(false);
    let now_s = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    // Use the same completion evidence as the scraper's download gate.
    let completed = prior.as_ref().is_some_and(|c| c["isCompleted"] == true)
        || pe_trackers::library::completion::is_finished(event, now_s);
    upsert(
        &mut tx,
        "CalendarEvents",
        json!({"id":eid,"name":calendar_name,"source":source,"externalUrl":calendar_url,"scrapedOriginalId":calendar_original_id,"approximateStartTime":calendar_start,"approximateStartTime_utc":calendar_start,"approximateEndTime":calendar_end,"approximateEndTime_utc":calendar_end,"createdAt":now,"updatedAt":now,"isPrivate":private}),
    )?;
    upsert(
        &mut tx,
        "VesselParticipantGroups",
        json!({"id":group,"name":event.title,"calendarEventId":eid,"createdAt":now,"updatedAt":now}),
    )?;
    upsert(
        &mut tx,
        "Courses",
        json!({"id":course,"name":event.title,"calendarEventId":eid,"createdAt":now,"updatedAt":now}),
    )?;
    upsert(
        &mut tx,
        "CompetitionUnits",
        json!({"id":cid,"name":event.title,"calendarEventId":eid,"courseId":course,"vesselParticipantGroupId":group,"scrapedOriginalId":unit_original_id,"scrapedUrl":original_url,"startTime":start,"endTime":end,"isCompleted":completed,"status":if completed {"COMPLETED"}else{"INPROGRESS"},"createdAt":now,"updatedAt":now}),
    )?;
    for (i, g) in geometries.iter().enumerate() {
        let gid = guid(&format!("{key}/geometry/{i}"));
        upsert(
            &mut tx,
            "CourseUnsequencedUntimedGeometries",
            json!({"id":gid,"courseId":course,"geometryType":g["geometry"]["type"],"coordinates":g["geometry"]["coordinates"],"properties":g["properties"],"order":i,"createdAt":now,"updatedAt":now}),
        )?;
    }
    let old=tx.query(r#"SELECT row_to_json(p),row_to_json(v),EXISTS(SELECT 1 FROM public."VesselParticipantTrackJsons" t WHERE t."vesselParticipantId"=p.id AND t."competitionUnitId"=$2::text::uuid) FROM public."VesselParticipants" p JOIN public."Vessels" v ON v.id=p."vesselId" WHERE p."vesselParticipantGroupId"=$1::text::uuid ORDER BY p.id"#,&[&group,&cid]).doing("match","race participants")?;
    let mut participants: BTreeMap<String, Vec<Participant>> = BTreeMap::new();
    for row in old {
        let p: Value = row.get(0);
        let v: Value = row.get(1);
        if let Some(id) = v["vesselId"].as_str() {
            participants
                .entry(id.to_owned())
                .or_default()
                .push((p, v, row.get(2)));
        }
    }
    let mut count = 0;
    for boat in &event.boats {
        check(cancel)?;
        let prior = participant(
            participants
                .get(&boat.id)
                .map(Vec::as_slice)
                .unwrap_or_default(),
            &boat.name,
        )?;
        let vid = prior
            .and_then(|(_, v, _)| v["id"].as_str())
            .map(str::to_owned)
            .unwrap_or_else(|| guid(&format!("{key}/vessel/{}", boat.id)));
        let pid = prior
            .and_then(|(p, _, _)| p["id"].as_str())
            .map(str::to_owned)
            .unwrap_or_else(|| guid(&format!("{key}/participant/{}", boat.id)));
        upsert(
            &mut tx,
            "Vessels",
            json!({"id":vid,"vesselId":boat.id,"publicName":boat.name,"source":source,"sailNumber":boat.sail,"model":boat.model,"createdAt":now,"updatedAt":now}),
        )?;
        upsert(
            &mut tx,
            "VesselParticipants",
            json!({"id":pid,"vesselId":vid,"vesselParticipantId":boat.id,"vesselParticipantGroupId":group,"sailNumber":boat.sail,"createdAt":now,"updatedAt":now}),
        )?;
        if boat.fixes.is_empty() {
            continue;
        }
        let old=tx.query_opt(r#"SELECT id::text,"providedStorageKey","calculatedStorageKey","simplifiedStorageKey" FROM public."VesselParticipantTrackJsons" WHERE "competitionUnitId"=$1::text::uuid AND "vesselParticipantId"=$2::text::uuid ORDER BY id LIMIT 1"#,&[&cid,&pid]).doing("match","track identity")?;
        let tid = old
            .as_ref()
            .map(|r| r.get::<_, String>(0))
            .unwrap_or_else(|| guid(&format!("{key}/track/{}", boat.id)));
        let storage = old
            .as_ref()
            .map(|r| r.get::<_, String>(1))
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| format!("individual-tracks/{cid}/vessel/provided/{pid}.geojson"));
        let path = safe_file(root, &storage)?;
        let coordinates: Vec<_> = boat
            .fixes
            .iter()
            .map(|f| json!([f.lon, f.lat, 0, f.t * 1000, f.sog, f.cog]))
            .collect();
        let feature = json!({"type":"Feature","properties":{"vesselParticipantId":pid,"competitionUnitId":cid,"detail":{"lon":0,"lat":1,"elevation":2,"time":3,"sog":4,"cog":5}},"geometry":{"type":"LineString","coordinates":coordinates}});
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).doing("create", parent.display())?;
        }
        let bytes = serde_json::to_vec(&feature).doing("encode", "SYRF track")?;
        // Publish each complete file atomically before its DB reference. A failed transaction
        // leaves a complete retryable file, never a committed reference to a partial write.
        pe_core::io::write_atomic(&path, &bytes)?;
        upsert(
            &mut tx,
            "VesselParticipantTrackJsons",
            json!({"id":tid,"competitionUnitId":cid,"vesselParticipantId":pid,"providedStorageKey":storage,"calculatedStorageKey":old.as_ref().map(|r|r.get::<_,String>(2)).unwrap_or_default(),"simplifiedStorageKey":old.as_ref().map(|r|r.get::<_,String>(3)).unwrap_or_default(),"locationUpdateCount":boat.fixes.len()}),
        )?;
        // These three providers do not publish SYRF mark-rounding/crossing events.
        // Preserve existing VesselParticipantEvents; never manufacture them from endpoints.
        count += 1;
    }
    check(cancel)?;
    tx.commit().doing("commit", "scraped race")?;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn historical_reused_boat_ids_use_the_actual_race_track() {
        let candidates = vec![
            (
                json!({"id":"wrong"}),
                json!({"publicName":"Old name"}),
                false,
            ),
            (
                json!({"id":"right"}),
                json!({"publicName":"Correct boat"}),
                true,
            ),
        ];
        assert_eq!(
            participant(&candidates, "Renamed boat").unwrap().unwrap().0["id"],
            "right"
        );
        let ambiguous = vec![
            (json!({"id":"one"}), json!({"publicName":"Same"}), true),
            (json!({"id":"two"}), json!({"publicName":"Same"}), true),
        ];
        assert!(participant(&ambiguous, "Same").is_err());
    }
}
