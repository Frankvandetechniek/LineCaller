// Settings lock: the code the user must enter before the Settings window
// opens. Hard-coded for now; keep every check in this file so it can become
// configurable later.
//
// This is only a casual deterrent against accidental changes on a studio
// computer. The code ships inside the app and anyone with access to the
// configuration files can still change them.

/** Number of digits in the settings code. */
export const SETTINGS_PIN_LENGTH = 4;
const SETTINGS_PIN = "1217";

/** True when `entry` is the settings code. */
export function checkSettingsPin(entry: string): boolean {
  return entry === SETTINGS_PIN;
}
