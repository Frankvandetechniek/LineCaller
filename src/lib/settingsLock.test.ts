import { describe, expect, it } from "vitest";
import { SETTINGS_PIN_LENGTH, checkSettingsPin } from "./settingsLock";

describe("settings lock", () => {
  it("accepts only the 4-digit settings code", () => {
    expect(SETTINGS_PIN_LENGTH).toBe(4);
    expect(checkSettingsPin("1217")).toBe(true);
    for (const wrong of ["", "1216", "121", "12170", "7121", " 1217"]) expect(checkSettingsPin(wrong)).toBe(false);
  });
});
