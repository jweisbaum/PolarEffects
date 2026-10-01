# Data sources and acknowledgements

PolarExplorer embeds the ORC catalogue and Natural Earth map. Weather and
tracker data are requested by the user. Sampled environmental values retain
their dataset identifier and version in the project. These notices describe
the sources used by this release; they do not change the providers' terms.

## ORC catalogue

The embedded catalogue is built from [jieter/orc-data](https://github.com/jieter/orc-data).
Copyright (c) 2015, 2016, 2017 Jan Pieter Waagmeester. Licensed under MIT; the
complete [copyright and permission notice](licenses/orc-data-MIT.txt) accompanies
the application. The native About panel records the embedded catalogue's
commit, source date, build date and certificate count. Its source polars are
based on ORC certificates; inclusion does not imply endorsement by ORC.

## ORR catalogue

Complete public certificates, ratings and offshore/short-course polar tables
come from [RegattaMan’s ORR valid list](https://www.regattaman.com/valid_list_ora.php?crule=ORR&sdir=true&ssdir=true&sort=3&ssort=0).
The bundled 2026 snapshot contains 300 certificates and 600 table variants,
captured 2026-09-30. Settings can refresh a chosen year. The snapshot preserves
all named list columns and certificate fields, public ownership and comments,
hull/rig/sail/stability/trim measurements, performance metrics, drawing
parameters, every embedded rating block, and original speed/time-allowance
tables. Rating values come from the page's embedded JSON, not its initially
empty display inputs. Only normalized certificate data is stored, not raw HTML,
page scripts or login/session state.
See `assets/orr/README.md` for the snapshot’s provenance and rebuild command.

## Wind and waves: ERA5

Contains modified Copernicus Climate Change Service information. ERA5 is
produced by ECMWF for the Copernicus Climate Change Service (C3S). PolarExplorer
interpolates archive values to track positions and converts units; these
derived samples are not an official ECMWF product.

- [ERA5 hourly data on single levels](https://cds.climate.copernicus.eu/datasets/reanalysis-era5-single-levels?tab=overview)
  provides the underlying wind and wave data, dataset documentation and licence.
- [WeatherBench2](https://weatherbench2.readthedocs.io/en/latest/data-guide.html)
  supplies the hourly 0.25° ERA5 archive through 2023-01-10 used for historical
  wind sampling (`1959-2023_01_10-full_37-1h-0p25deg-chunk-1.zarr`).
- [ARCO-ERA5](https://github.com/google-research/arco-era5) supplies the
  `full_37-1h-0p25deg-chunk-1.zarr-v3` archive used for waves and the wind fallback.
  Both archives are hosted on Google Cloud. The archive software's licence
  does not replace the underlying ERA5 data terms.

Neither the European Commission nor ECMWF is responsible for PolarExplorer'
use of Copernicus information or for its derived results.

## Currents: Copernicus Marine

Generated using E.U. Copernicus Marine Service Information. Product identifiers
and DOI links for the current sources supported by PolarExplorer are:

| Product | Use | DOI |
| --- | --- | --- |
| `NWSHELF_MULTIYEAR_PHY_004_009` | North-West European Shelf reanalysis | [10.48670/moi-00059](https://doi.org/10.48670/moi-00059) |
| `IBI_MULTIYEAR_PHY_005_002` | Iberia–Biscay–Ireland reanalysis | [10.48670/moi-00029](https://doi.org/10.48670/moi-00029) |
| `GLOBAL_ANALYSISFORECAST_PHY_001_024` | Global merged surface currents | [10.48670/moi-00016](https://doi.org/10.48670/moi-00016) |
| `MULTIOBS_GLO_PHY_MYNRT_015_003` | GlobCurrent multiyear and near-real-time total currents | [10.48670/mds-00327](https://doi.org/10.48670/mds-00327) |

The product pages provide their scientific references and licence. When
sharing derived work, cite the products actually used and retain the sample
provenance. See the service's [citation guidance](https://help.marine.copernicus.eu/en/articles/4444611-citing-copernicus-marine-products-and-services).

## Basemap and imported tracks

The bundled land and coastline geometry comes from [Natural Earth](https://www.naturalearthdata.com/about/terms-of-use/),
whose map data are in the public domain. No map tiles are downloaded.

YellowBrick, Geovoile and Blue Water Tracks provide user-selected race tracks.
Their event data and any files you import remain subject to the rights and
terms of their respective providers. PolarExplorer' application licence does
not grant rights to third-party event data.

The optional SYRF library uses the user's PostgreSQL database and local
GeoJSON files. Its schema and native scraper mapping follow
[syrf-schema](https://github.com/sailing-yacht-research-foundation/syrf-schema),
[tracker-scraper](https://github.com/sailing-yacht-research-foundation/tracker-scraper)
and the individual-track formatter in
[raw-data-server](https://github.com/sailing-yacht-research-foundation/raw-data-server).
Original race URLs and source identifiers accompany searchable tracks.
Scraping uses public YellowBrick, Geovoile and Blue Water tracks in Rust;
it does not run the upstream Node or browser scrapers. YellowBrick catalogue
discovery uses configured, user-authorized mobile credentials to resolve race
codes and associate only products explicitly listed as free. Credentials are
local settings, never bundled into builds, logged or included in metadata.
The metadata snapshot is limited to the selected provider families. Full SQL
export covers all database records, and is separate from the GeoJSON files.
