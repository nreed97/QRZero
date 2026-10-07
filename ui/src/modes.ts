// Modes offered in the entry screen, mapped to ADIF MODE/SUBMODE.
export interface ModeChoice { label: string; mode: string; submode?: string; rst: string }

export const MODES: ModeChoice[] = [
  { label: "CW", mode: "CW", rst: "599" },
  { label: "SSB", mode: "SSB", rst: "59" },
  { label: "FT8", mode: "FT8", rst: "-10" },
  { label: "FT4", mode: "MFSK", submode: "FT4", rst: "-10" },
  { label: "RTTY", mode: "RTTY", rst: "599" },
  { label: "PSK31", mode: "PSK", submode: "PSK31", rst: "599" },
  { label: "JS8", mode: "MFSK", submode: "JS8", rst: "-10" },
  { label: "Q65", mode: "MFSK", submode: "Q65", rst: "-10" },
  { label: "JT65", mode: "JT65", rst: "-10" },
  { label: "MSK144", mode: "MSK144", rst: "0" },
  { label: "OLIVIA", mode: "OLIVIA", rst: "599" },
  { label: "AM", mode: "AM", rst: "59" },
  { label: "FM", mode: "FM", rst: "59" },
  { label: "DIGITALVOICE", mode: "DIGITALVOICE", rst: "59" },
  { label: "SSTV", mode: "SSTV", rst: "595" },
];

export function modeLabel(f: Record<string, string>): string {
  return f.SUBMODE || f.MODE || "";
}

export function choiceFor(label: string): ModeChoice {
  return MODES.find((m) => m.label === label) ?? { label, mode: label, rst: "59" };
}

export const BANDS: [string, number, number][] = [
  ["160m", 1.8, 2.0], ["80m", 3.5, 4.0], ["60m", 5.06, 5.45], ["40m", 7.0, 7.3], ["30m", 10.1, 10.15],
  ["20m", 14.0, 14.35], ["17m", 18.068, 18.168], ["15m", 21.0, 21.45], ["12m", 24.89, 24.99],
  ["10m", 28.0, 29.7], ["6m", 50.0, 54.0], ["4m", 70.0, 71.0], ["2m", 144.0, 148.0],
  ["1.25m", 222.0, 225.0], ["70cm", 420.0, 450.0], ["33cm", 902.0, 928.0], ["23cm", 1240.0, 1300.0],
  ["2190m", 0.1357, 0.1378], ["630m", 0.472, 0.479],
];

export function bandForFreq(mhz: number): string | undefined {
  return BANDS.find(([, lo, hi]) => mhz >= lo && mhz <= hi)?.[0];
}
