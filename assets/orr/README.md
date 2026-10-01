# ORR public polar snapshot

Captured on 2026-09-30 from the [RegattaMan 2026 ORR valid list](https://www.regattaman.com/valid_list_ora.php?crule=ORR&sdir=true&ssdir=true&sort=3&ssort=0&yr=2026).
The snapshot contains 300 certificates, each with offshore and short-course
boat-speed tables (600 distinct certificate/variant keys). Speeds are knots;
dimensions are the original metric measurements. Each variant also includes
the complete public certificate payload: named valid-list columns, all data
fields (including ownership and comments), performance metrics, line-drawing
parameters, every embedded rating block, and all four original speed and
time-allowance tables. Ratings retain their system, type, course, wind band,
spin/non-spin values and published decimal precision. No raw HTML, page scripts
or login/session state is retained.

The completed snapshot has 77,097 certificate fields and 18,360 rating entries
across 300 unique certificates, plus 21 list columns and four source tables
per certificate. All 600 original boat-speed grids are unchanged. The generated
JSON is compact to avoid shipping whitespace around the complete payload.

Regenerate with `CARGO_INCREMENTAL=0 cargo run -p pe-trackers --example scrape_orr -- 2026 assets/orr/catalogue.json`.
Settings can download additional years or refresh certificates; repeated keys
replace their catalogue entry and never alter copies already in projects.
