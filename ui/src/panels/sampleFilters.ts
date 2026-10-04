import type { TrackFilters } from "../generated/TrackFilters";

export const NO_SAMPLE_FILTERS: TrackFilters = {
  time_start: null, time_end: null, min_bsp: null, max_bsp: null,
  max_awa_change: null, max_wind_speed_change: null, max_wind_direction_change: null,
  max_heading_change: null, heading_from: null, heading_to: null, cog_from: null, cog_to: null,
  vmg_min: null, vmg_max: null, twd_from: null, twd_to: null, exclude_tacks: false,
  tws_min: null, tws_max: null, twa_min: null, twa_max: null,
  hs_min: null, hs_max: null, current_min: null, current_max: null,
  wave_mode: "off", wave_sectors: [], wave_min: null, wave_max: null,
  wave_from: null, wave_to: null, exclude_no_tide: false,
  exclude_unknown_wave: false, exclude_unknown_current: false,
};
