import { mkdir, writeFile, readFile, readdir } from "node:fs/promises";
import { createServer } from "node:http";
import { join } from "node:path";
import { assert, newProject, blockWeatherDownloads } from "./harness.mjs";
const base = new URL("../../../crates/pe-trackers/tests/fixtures/yellowbrick/", import.meta.url);

const requests = [];
export default {
  name: "track library",
  // A recorded YellowBrick race for the scraper, served on loopback.
  async setup() {
    const routes = new Map([
      ["/JSON/rmsr2024/RaceSetup", await readFile(new URL("rmsr2024-RaceSetup.json", base))],
      ["/BIN/rmsr2024/AllPositions3", await readFile(new URL("rmsr2024-AllPositions3-first3.bin", base))],
    ]);
    const server = createServer((req, res) => { const path = req.url.split("?")[0]; const body = routes.get(path); requests.push(path);
      setTimeout(() => { res.writeHead(body ? 200 : 404); res.end(body); }, path.startsWith("/BIN/") ? 3000 : 0); });
    await new Promise(done => server.listen(0, "127.0.0.1", done));
    return { env: { PE_DRIVER_YELLOWBRICK: `http://127.0.0.1:${server.address().port}` }, teardown: () => new Promise(done => server.close(done)) };
  },
  async run(t) {
    const d = t.driver;
    await blockWeatherDownloads(d);
    const geo = join(d.automationRoot, "geojson");
    const metadata = join(d.automationRoot, "metadata");
    await mkdir(geo, { recursive: true }); await mkdir(metadata, { recursive: true });
    const hit = { id: "race/participant", vessel_id: "vessel", participant_id: "participant", competition_id: "race", boat_name: "Lurline", sail_number: "USA 43703", model: "Tripp 47", source: "YELLOWBRICK", event_name: "Pacific race", original_url: "Pacific race original URL", start: "2025-07-26T12:00:00Z", end: null, tracker_boat_id: "3", storage_key: "race.geojson", file_available: false };
    const vessel = { id: "vessel", publicName: "Lurline", model: "Tripp 47", class: "IRC", make: "Custom", builder: "Lübeck Yachts", sailNumber: "USA 43703" };
    const second = { ...hit, id: "race2/participant2", competition_id: "race2", participant_id: "participant2", event_name: "Coastal race", storage_key: "race2.geojson" };
    await writeFile(join(metadata, "boat-metadata.json"), JSON.stringify({ version: 1, tables: { Vessels: [vessel] }, tracks: [hit, second] }));
    await writeFile(join(geo, "race.geojson"), JSON.stringify({ type: "Feature", properties: { vesselParticipantId: "participant", competitionUnitId: "race", detail: { lon: 0, lat: 1, elevation: 2, time: 3, sog: 4, cog: 5 } }, geometry: { type: "LineString", coordinates: [[-122, 35, 0, 1753531200000, 7, 270], [-122.05, 35, 0, 1753531800000, 8, 270], [-122.1, 35, 0, 1753532400000, 9, 270]] } }));
    await writeFile(join(geo, "race2.geojson"), (await readFile(join(geo, "race.geojson"), "utf8")).replaceAll('"participant"', '"participant2"').replaceAll('"race"', '"race2"'));
    await newProject(d, "Library tracks");
    assert.equal(await d.exists('.boat-track-search'), false, "search is hidden without a metadata file");
    assert.equal(await d.exists('[data-feature="tracks:boat-search"]'), false);
    await t.shot("no-metadata-search-hidden");
    await d.click('[data-feature="shell:settings"]');
    await d.type('[data-feature="settings:data-source"]', "whirlwind");
    // Scraping is back, into files only (asked 2026-10-04): nothing about a database.
    await d.waitFor('[data-feature="settings:library-geojson"]');
    for (const gone of ["settings:db-host", "settings:db-test", "settings:db-scrape", "settings:db-export", "settings:db-download"]) {
      assert.equal(await d.exists(`[data-feature="${gone}"]`), false, `${gone} is gone`);
    }
    for (const control of ["settings:library-schedule", "settings:library-yb-user-key", "settings:library-yb-device-id", "settings:library-urls", "settings:library-scrape"]) {
      assert.ok(await d.exists(`[data-feature="${control}"]`), `${control} is there`);
    }
    assert.ok((await d.text('.library-settings')).includes("Only finished races are scraped."));
    // The SYRF database's read-only metadata download (asked 2026-10-06).
    // Pointed at a closed port, so no test ever reaches a real database.
    for (const control of ["settings:library-db-host", "settings:library-db-port", "settings:library-db-name", "settings:library-db-user",
      "settings:library-db-password", "settings:library-db-tls", "settings:library-db-test", "settings:library-db-download", "settings:library-db-cancel"]) {
      assert.ok(await d.exists(`[data-feature="${control}"]`), `${control} is there`);
    }
    assert.ok((await d.text('.library-settings')).includes("Nothing is written to the database"));
    await d.type('[data-feature="settings:library-db-host"]', "127.0.0.1");
    await d.type('[data-feature="settings:library-db-port"]', "1");
    await d.run(`document.querySelector('[data-feature="settings:library-db-test"]').scrollIntoView({block: "center"}); done(true);`);
    await d.click('[data-feature="settings:library-db-test"]');
    await d.waitFor(".database-connection.failed", { text: "Connection failed" });
    assert.ok((await d.text('.library-settings [role="alert"]')).includes("PostgreSQL connection failed"));
    await t.shot("library-database");
    // Closing Settings saves what was typed, without the Save button (asked 2026-10-06).
    await d.type('[data-feature="settings:library-geojson"]', geo);
    await d.click('[data-feature="settings:close"]');
    await d.waitGone('[data-feature="settings:library-geojson"]');
    const closed = JSON.parse(await readFile(join(d.automationRoot, "config", "settings.json"), "utf8"));
    assert.equal(closed.library.geojson_directory, geo, "closing saved the GeoJSON folder");
    await d.click('[data-feature="shell:settings"]');
    await d.waitFor('[data-feature="settings:library-geojson"]');
    assert.equal(await d.run(`done(document.querySelector('[data-feature="settings:library-geojson"]').value);`), geo);
    await d.type('[data-feature="settings:library-metadata"]', metadata);
    await d.run(`document.querySelector('[data-feature="settings:library-save"]').scrollIntoView({block: "center"}); done(true);`);
    await d.click('[data-feature="settings:library-save"]');
    await d.waitFor(".library-settings", { text: "Library settings saved" });
    await t.shot("library-settings");
    const saved = JSON.parse(await readFile(join(d.automationRoot, "config", "settings.json"), "utf8"));
    assert.equal(saved.library.geojson_directory, geo);
    assert.equal(saved.library.metadata_directory, metadata);
    assert.equal(saved.database, undefined, "the connection is kept under library, not a database section");
    assert.equal(saved.library.database.host, "127.0.0.1");
    assert.equal(saved.library.database.port, 1);
    await d.click('[data-feature="settings:close"]');
    await d.waitFor('[data-feature="tracks:boat-search"]');
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
    assert.equal(await d.exists('[role="dialog"]'), false);
    await d.waitFor('.statusbar', { text: "Whirlwind cache path is not a regular directory" });
    await d.waitGone('.busy-spinner.on');
    assert.equal(await d.count('[data-feature="tracks:select"]:checked'), 0, "queuing weather clears the selection");
    await t.shot("weather-for-all-imported-tracks");
    await d.click('[data-feature="shell:settings"]');
    await d.waitFor('[data-feature="settings:library-urls"]');
    await d.run(`document.querySelector('[data-feature="settings:library-urls"]').scrollIntoView({block: "center"}); done(true);`);
    // A manual scrape of one finished race: the status bar follows it, and
    // the race lands in the folders and the metadata.
    await d.type('[data-feature="settings:library-urls"]', "https://yb.tl/rmsr2024");
    await d.click('[data-feature="settings:library-scrape"]');
    // The fixture server holds the positions back, so the scrape is seen running.
    await d.waitFor('[data-feature="shell:scrape-status"]', { timeoutMs: 15000 });
    await t.shot("scrape-status-bar");
    await d.waitFor(".library-scrape-status", { text: "Scrape finished", timeoutMs: 60000 });
    await t.shot("scrape-finished");
    const status = await d.text(".library-scrape-status");
    assert.ok(/Tracks: [1-9]/.test(status), `tracks were saved: ${status}`);
    const after = JSON.parse(await readFile(join(metadata, "boat-metadata.json"), "utf8"));
    assert.ok(after.tracks.length > 2, "the scraped boats are searchable");
    assert.ok(after.tables.CompetitionUnits.some(u => u.approximateStartLocation), "the race has a start");
    assert.ok((await readdir(join(geo, "individual-tracks"))).length > 0, "the tracks are files");
    // Scraped again: the library has it, so nothing is fetched (asked 2026-10-05).
    const fetched = requests.length;
    await d.click('[data-feature="settings:library-scrape"]');
    await d.waitFor(".library-scrape-status", { text: "already in the library: 1", timeoutMs: 30000 });
    await d.waitFor(".library-scrape-status", { text: "Scrape finished", timeoutMs: 30000 });
    assert.ok((await d.text(".library-scrape-status")).includes("Tracks: 0"));
    assert.equal(requests.length, fetched, "no request for a race the library holds");
    await t.shot("scrape-again-skips-held");
  },
};
