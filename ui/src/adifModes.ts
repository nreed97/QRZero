/** The ADIF MODE enumeration with its SUBMODE values (ADIF 3.1), for adding modes in Settings. */
export const ADIF_MODES: Record<string, string[]> = {
  AM: [],
  ARDOP: [],
  ATV: [],
  C4FM: [],
  CHIP: ["CHIP64", "CHIP128"],
  CLO: [],
  CONTESTI: [],
  CW: ["PCW"],
  DIGITALVOICE: ["C4FM", "DMR", "DSTAR", "FREEDV", "M17"],
  DOMINO: ["DOM-M", "DOM4", "DOM5", "DOM8", "DOM11", "DOM16", "DOM22", "DOM44", "DOM88", "DOMINOEX", "DOMINOF"],
  DYNAMIC: ["VARA HF", "VARA SATELLITE", "VARA FM 1200", "VARA FM 9600"],
  FAX: [],
  FM: [],
  FSK441: [],
  FT8: [],
  HELL: ["FMHELL", "FSKH105", "FSKH245", "FSKHELL", "HELL80", "HELLX5", "HELLX9", "SLOWHELL"],
  ISCAT: ["ISCAT-A", "ISCAT-B"],
  JT4: ["JT4A", "JT4B", "JT4C", "JT4D", "JT4E", "JT4F", "JT4G"],
  JT6M: [],
  JT9: ["JT9-1", "JT9-2", "JT9-5", "JT9-10", "JT9-30", "JT9A", "JT9B", "JT9C", "JT9D", "JT9E", "JT9E FAST", "JT9F", "JT9F FAST", "JT9G", "JT9G FAST", "JT9H", "JT9H FAST"],
  JT44: [],
  JT65: ["JT65A", "JT65B", "JT65B2", "JT65C", "JT65C2"],
  JTMS: [],
  MFSK: ["FST4", "FST4W", "FT4", "JS8", "JTMS", "MFSK4", "MFSK8", "MFSK11", "MFSK16", "MFSK22", "MFSK31", "MFSK32", "MFSK64", "MFSK64L", "MFSK128", "MFSK128L", "Q65"],
  MSK144: [],
  MT63: [],
  OLIVIA: ["OLIVIA 4/125", "OLIVIA 4/250", "OLIVIA 8/250", "OLIVIA 8/500", "OLIVIA 16/500", "OLIVIA 16/1000", "OLIVIA 32/1000"],
  OPERA: ["OPERA-BEACON", "OPERA-QSO"],
  PAC: ["PAC2", "PAC3", "PAC4"],
  PAX: ["PAX2"],
  PKT: [],
  PSK: ["FSK31", "PSK10", "PSK31", "PSK63", "PSK63F", "PSK125", "PSK250", "PSK500", "PSK1000", "PSKAM10", "PSKAM31", "PSKAM50", "PSKFEC31", "QPSK31", "QPSK63", "QPSK125", "QPSK250", "QPSK500", "SIM31"],
  PSK2K: [],
  Q15: [],
  QRA64: ["QRA64A", "QRA64B", "QRA64C", "QRA64D", "QRA64E"],
  ROS: ["ROS-EME", "ROS-HF", "ROS-MF"],
  RTTY: ["ASCI"],
  RTTYM: [],
  SSB: ["LSB", "USB"],
  SSTV: [],
  T10: [],
  THOR: ["THOR-M", "THOR4", "THOR5", "THOR8", "THOR11", "THOR16", "THOR22", "THOR25X4", "THOR50X1", "THOR50X2", "THOR100"],
  THRB: ["THRBX", "THRBX1", "THRBX2", "THRBX4", "THROB1", "THROB2", "THROB4"],
  TOR: ["AMTORFEC", "GTOR"],
  V4: [],
  VOI: [],
  WINMOR: [],
  WSPR: [],
};

export type Group = "cw" | "phone" | "digital";

/** The usual award column for an ADIF mode or submode. */
export function adifGroup(mode: string, submode = ""): Group {
  const m = (submode || mode).toUpperCase();
  if (m === "CW" || m === "PCW") return "cw";
  return ["AM", "FM", "SSB", "USB", "LSB", "DIGITALVOICE", "C4FM", "DMR", "DSTAR", "FREEDV", "M17", "ATV"].includes(m) ? "phone" : "digital";
}

/** The report usually exchanged in a mode. */
export function adifRst(mode: string, submode = ""): string {
  const m = (submode || mode).toUpperCase();
  if (["CW", "PCW", "RTTY", "RTTYM", "PSK", "OLIVIA", "DOMINO", "THOR", "MFSK", "HELL", "MT63", "PKT"].includes(mode.toUpperCase()) && !["FT4", "JS8", "Q65", "FST4", "FST4W", "JTMS"].includes(m)) return "599";
  if (["AM", "FM", "SSB", "DIGITALVOICE", "ATV"].includes(mode.toUpperCase())) return "59";
  if (mode.toUpperCase() === "SSTV") return "595";
  if (mode.toUpperCase() === "MSK144") return "0";
  return "-10";
}
