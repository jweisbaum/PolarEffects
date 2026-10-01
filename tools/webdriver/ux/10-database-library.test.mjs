import { mkdir, writeFile, readFile } from "node:fs/promises";
import { join } from "node:path";
import { assert, newProject } from "./harness.mjs";

export default {
  name: "database boat library",
  async run(t) {
    const d = t.driver;
    const geo = join(d.automationRoot, "geojson");
    const metadata = join(d.automationRoot, "metadata");
    await mkdir(geo, { recursive: true }); await mkdir(metadata, { recursive: true });
    const hit = { id: "race/participant", vessel_id: "vessel", participant_id: "participant", competition_id: "race", boat_name: "Lurline", sail_number: "USA 43703", model: "Tripp 47", source: "YELLOWBRICK", event_name: "Pacific race", original_url: "Pacific race original URL", start: "2025-07-26T12:00:00Z", end: null, tracker_boat_id: "3", storage_key: "race.geojson", file_available: false };
    const vessel = { id: "vessel", publicName: "Lurline", model: "Tripp 47", class: "IRC", make: "Custom", builder: "Lübeck Yachts", sailNumber: "USA 43703" };
    const second = { ...hit, id: "race2/participant2", competition_id: "race2", participant_id: "participant2", event_name: "Coastal race", storage_key: "race2.geojson" };
    await writeFile(join(metadata, "boat-metadata.json"), JSON.stringify({ version: 1, tables: { Vessels: [vessel] }, tracks: [hit, second] }));
    await writeFile(join(geo, "race.geojson"), JSON.stringify({ type: "Feature", properties: { vesselParticipantId: "participant", competitionUnitId: "race", detail: { lon: 0, lat: 1, elevation: 2, time: 3, sog: 4, cog: 5 } }, geometry: { type: "LineString", coordinates: [[-122, 35, 0, 1753531200000, 7, 270], [-122.05, 35, 0, 1753531800000, 8, 270], [-122.1, 35, 0, 1753532400000, 9, 270]] } }));
    await writeFile(join(geo, "race2.geojson"), (await readFile(join(geo, "race.geojson"), "utf8")).replaceAll('"participant"', '"participant2"').replaceAll('"race"', '"race2"'));
    await newProject(d, "Database tracks");
    await d.click('[data-feature="shell:settings"]');
    await d.waitFor('[data-feature="settings:db-host"]');
    assert.ok((await d.text('.database-settings')).includes('Only finished races are scraped. Ongoing, future and unverified races are skipped.'));
    await d.run(`document.querySelector('[data-feature="settings:db-schedule"]').scrollIntoView({block: "center"}); done(true);`);
    await t.shot("finished-races-policy");
    await d.type('[data-feature="settings:yb-user-key"]', "fixture-user-key");
    await d.type('[data-feature="settings:yb-device-id"]', "fixture-device-id");
    assert.equal(await d.run(`done(document.querySelector('[data-feature="settings:yb-user-key"]').type);`), "password");
    assert.equal(await d.run(`done(document.querySelector('[data-feature="settings:yb-device-id"]').type);`), "password");
    await d.run(`document.querySelector('[data-feature="settings:yb-user-key"]').scrollIntoView({block: "center"}); done(true);`);
    await t.shot("yellowbrick-credentials");
    await d.type('[data-feature="settings:db-geojson"]', geo);
    await d.type('[data-feature="settings:db-metadata"]', metadata);
    await d.type('[data-feature="settings:db-host"]', "127.0.0.1");
    await d.type('[data-feature="settings:db-port"]', "1");
    await d.click('[data-feature="settings:db-test"]');
    await d.waitFor(".database-connection.failed", { timeoutMs: 15000 });
    await t.shot("connection-failure");
    // Optional read-only local connection check for the developer's integration run.
    if (process.env.PE_TEST_SYRF_DB) {
      await d.type('[data-feature="settings:db-port"]', "5432");
      await d.type('[data-feature="settings:db-name"]', process.env.PE_TEST_SYRF_DB);
      await d.click('[data-feature="settings:db-test"]');
      await d.waitFor(".database-connection.ok", { timeoutMs: 15000 });
      await t.shot("connection-success");
    }
    await d.click('[data-feature="settings:db-save"]');
    await d.waitFor(".database-settings", { text: "Database settings saved" });
    const saved = JSON.parse(await readFile(join(d.automationRoot, "config", "settings.json"), "utf8"));
    assert.equal(saved.database.geojson_directory, geo);
    assert.equal(saved.database.metadata_directory, metadata);
    assert.equal(saved.database.scrape_schedule, "on_demand");
    assert.equal(saved.database.yellowbrick_user_key, "fixture-user-key");
    assert.equal(saved.database.yellowbrick_device_id, "fixture-device-id");
    await d.click('[data-feature="settings:close"]');
    await d.type('[data-feature="tracks:boat-search"]', "lurl");
    await d.waitFor(".boat-track-results li", { text: "Lurline" });
    await t.shot("boat-search");
    for (const query of ["tripp", "irc", "custom", "lubeck", "tripp lubeck irc"]) {
      // Wait for a different result first; otherwise the previous matching row
      // can satisfy waitFor before React starts the next debounced search.
      await d.type('[data-feature="tracks:boat-search"]', "no-such-vessel-xyz");
      await d.waitFor('.boat-track-search [role="status"]', { text: "0 matching tracks" });
      await d.type('[data-feature="tracks:boat-search"]', query);
      await d.waitFor(".boat-track-results li", { text: "Lurline" });
      assert.equal(await d.count(".boat-track-results li"), 2);
    }
    await t.shot("vessel-fields-search");
    await d.click('[data-feature="tracks:boat-import"]');
    await d.waitFor('[data-feature="tracks:fetch-weather"]', { timeoutMs: 15000 });
    await t.shot("imported-track-weather");
    assert.ok((await d.text(".track-list")).includes("Lurline"));
    assert.equal(await d.run(`done(document.querySelector('[data-feature="tracks:boat-search"]').value);`), "tripp lubeck irc");
    await d.waitFor('.boat-track-search [role="status"]', { text: "1 matching track" });
    assert.equal(await d.count(".boat-track-results li"), 1, "only the imported result is removed");
    assert.equal(await d.text(".boat-track-results li > span"), "Coastal race");
    await d.click('[data-feature="tracks:boat-import"]');
    await d.waitFor('.track-list li:nth-child(2) [data-feature="tracks:fetch-weather"]');
    await d.waitFor('.boat-track-search [role="status"]', { text: "0 matching tracks" });
    assert.equal(await d.count(".boat-track-results li"), 0);
    assert.equal(await d.text('[data-feature="tracks:boat-search"]'), "tripp lubeck irc");
    await t.shot("both-imported-results-removed");
    await d.click('[data-feature="tracks:select-all"]');
    assert.equal(await d.count('[data-feature="tracks:select"]:checked'), 2);
    await d.waitFor('[data-feature="tracks:fetch-weather-selected"]', { text: "Fetch weather for 2 selected tracks…" });
    await d.run(`document.querySelector('[data-feature="tracks:select-all"]').scrollIntoView({block: "center"}); done(true);`);
    await t.shot("all-imported-tracks-selected");
    await d.click('[data-feature="tracks:fetch-weather-selected"]');
    await d.waitFor('[role="dialog"]', { text: "6 samples" });
    await t.shot("weather-for-all-imported-tracks");
    await d.key("Escape");
    await d.waitGone('[role="dialog"]');
    assert.equal(await d.count('[data-feature="tracks:select"]:checked'), 2, "cancelling the download keeps the selection");
  },
};
