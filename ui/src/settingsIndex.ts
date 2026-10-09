import { AWARD_LIST } from "./awardsPref";

/** One searchable setting. `find` is the text of the label/legend to jump to on its page. */
export interface SettingEntry {
  tab: string;
  label: string;
  find?: string;
  words?: string;
}

const E = (tab: string, label: string, words = "", find = label): SettingEntry => ({ tab, label, find, words });

/** Everything findable from the Settings search box (besides the section names). */
export const SETTING_ENTRIES: SettingEntry[] = [
  E("station", "Add callsign", "station call operator old"),
  E("locations", "Add location", "qth grid home park pota sota"),
  E("logs", "New log", "create rename delete switch operator"),
  E("fields", "Entry fields", "qso panel layout line sticky keep default label width"),
  E("lookup", "Look up callsigns on QRZ.com", "qrz xml login lookup"),
  E("lookup", "QRZ username", "login"),
  E("lookup", "QRZ password", "login secret"),
  E("radios", "Listen to WSJT-X / JTDX", "wsjtx jtdx udp ft8 ft4"),
  E("radios", "Listen on (one per port, comma between)", "wsjtx port 2237"),
  E("radios", "Multicast group (optional)", "wsjtx"),
  E("radios", "Pass on to (GridTracker, JTAlert …)", "forward gridtracker jtalert"),
  E("radios", "Log QSOs that WSJT-X / JTDX log", "auto log wsjtx"),
  E("radios", "Listen to N1MM Logger+", "n1mm contest udp"),
  E("radios", "Copy QSOs logged in N1MM into this log", "n1mm auto log contest"),
  E("radios", "QSOs logged by other programs", "auto lookup qrz fill"),
  E("radios", "Rotator", "rotor pstrotator rotctld gs-232 yaesu serial baud antenna"),
  E("radios", "Control a rotator", "rotor enable"),
  E("radios", "Let N1MM (or another program) turn the rotator through QRZero", "rotor n1mm serve"),
  E("radios", "Country file", "cty dxcc prefix download ad1c"),
  E("udp", "UDP connections", "relay forward custom message n1mm log4om"),
  E("startup", "Startup programs", "launch start wsjtx n1mm programs"),
  E("cluster", "Connect when QRZero starts", "dx cluster telnet auto connect"),
  E("cluster", "Alerts for needed spots", "sound popup needed beep"),
  E("cluster", "Play a sound when a needed station is spotted", "alert beep needed"),
  E("cluster", "Show a popup", "alert needed spot tune"),
  E("cluster", "Commands after login", "cluster node telnet password"),
  E("awards", "CW club awards", "skcc cwops naqcc fists"),
  ...AWARD_LIST.map((a) => E("awards", a.name, `award hide show ${a.what}`)),
  E("backups", "Automatic backups", "backup keep newest copy"),
  E("backups", "Backups folder", "backup location path"),
  E("backups", "Restore", "backup restore file"),
  E("general", "Colours", "colors theme dark light"),
  E("general", "Text size", "font zoom"),
  E("general", "Distances", "units km miles"),
  E("general", "Dates in the log", "format date"),
  E("general", "Frequencies in the log", "format mhz khz"),
  E("general", "Show the computer's local time next to the UTC clock", "time clock utc"),
  E("general", "Reports filled in for each mode", "rst 599 59 signal report default"),
  E("general", "Ask before deleting", "confirm delete behaviour"),
  E("general", "Setup", "wizard setup again first run"),
];
