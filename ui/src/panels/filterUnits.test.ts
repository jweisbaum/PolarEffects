import { expect, it } from "vitest";

import { shownValue, speedUnit, storedValue, waveUnit } from "./filterUnits";

it("shows a stored filter in the display unit and stores what is typed back in knots and metres", () => {
  const kmh = speedUnit({ speed: "kmh", wave_height: "m", distance: "nm" });
  expect(kmh.symbol).toBe("km/h");
  // 1 kn = 1.852 km/h exactly.
  expect(shownValue(1, kmh.factor)).toBe("1.852");
  expect(storedValue("18.52", 1, kmh.factor)).toBeCloseTo(10, 12);
  const ms = speedUnit({ speed: "ms", wave_height: "m", distance: "nm" });
  // 1 m/s = 3600 / 1852 kn = 1.943844… kn.
  expect(storedValue("1", null, ms.factor)).toBeCloseTo(1.943844, 6);
  expect(shownValue(3600 / 1852, ms.factor)).toBe("1");
  const ft = waveUnit({ speed: "kn", wave_height: "ft", distance: "nm" });
  // 3 m = 9.843 ft (3 / 0.3048 = 9.84252).
  expect(shownValue(3, ft.factor)).toBe("9.843");
  expect(storedValue("10", null, ft.factor)).toBeCloseTo(3.048, 12);
  // Untouched text keeps the stored value exactly; empty clears; junk is refused.
  expect(storedValue("1.852", 1, kmh.factor)).toBe(1);
  expect(storedValue("0.514", 1, ms.factor)).toBe(1);
  expect(storedValue("9.843", 3, ft.factor)).toBe(3);
  expect(storedValue("  ", 1, kmh.factor)).toBeNull();
  expect(storedValue("abc", 1, kmh.factor)).toBeUndefined();
  // Knots and metres are unchanged.
  expect(shownValue(2.5)).toBe("2.5");
  expect(storedValue("2.5", null)).toBe(2.5);
});
