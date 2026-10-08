import { expect, test } from "@playwright/test";
import { qsoStage, type StageInput } from "../src/ftxStage";

const on = (tx_message: string, extra: Partial<StageInput> = {}) =>
  qsoStage({ tx_message, dx_call: "K1ABC", mycall: "N0CALL", tx_enabled: true, transmitting: false, ...extra });

test("QSO stage from the message being sent", () => {
  expect(on("CQ N0CALL EN34").text).toBe("Calling CQ");
  expect(on("CQ POTA N0CALL EN34").kind).toBe("cq");
  expect(on("CQ N0CALL").kind).toBe("cq");
  expect(on("K1ABC N0CALL EN34")).toEqual({ kind: "call", text: "Calling K1ABC", dx: "K1ABC" });
  expect(on("K1ABC N0CALL").text).toBe("Calling K1ABC");
  expect(on("K1ABC N0CALL -12")).toEqual({ kind: "report", text: "Sending report", dx: "K1ABC" });
  expect(on("K1ABC N0CALL +05").kind).toBe("report");
  expect(on("K1ABC N0CALL R-08").text).toBe("Sending R+report");
  expect(on("K1ABC N0CALL R+02").kind).toBe("rreport");
  expect(on("K1ABC N0CALL R EN34").text).toBe("Sending R+grid");
  expect(on("K1ABC N0CALL RRR").text).toBe("Sending RRR");
  expect(on("K1ABC N0CALL RR73")).toEqual({ kind: "rr73", text: "Sending RR73", dx: "K1ABC" });
  expect(on("K1ABC N0CALL 73").text).toBe("Sending 73");
  // A message to someone other than the DX call still names who it's for.
  expect(on("W9XYZ N0CALL -03").dx).toBe("W9XYZ");
});

test("QSO stage: idle, free text, compound and hashed calls", () => {
  expect(on("K1ABC N0CALL -12", { tx_enabled: false }).text).toBe("Idle");
  // Halted with Enable Tx off but still on the air counts as sending.
  expect(on("K1ABC N0CALL -12", { tx_enabled: false, transmitting: true }).kind).toBe("report");
  expect(on("").kind).toBe("idle");
  expect(on("TNX BOB 73 GL").text).toBe("Sending free text");
  expect(on("TNX 73 GL").kind).toBe("free");
  // Someone else's call in the sender's place: not ours, so not a QSO step.
  expect(on("K1ABC W9XYZ -12").kind).toBe("free");
  expect(on("CQ W9XYZ EN37").kind).toBe("free");
  expect(on("<PJ4/K1ABC> N0CALL R-10").text).toBe("Sending R+report");
  expect(on("K1ABC <N0CALL/P> RR73", { mycall: "N0CALL" }).kind).toBe("rr73");
  expect(on("K1ABC PJ4/N0CALL 73").kind).toBe("73");
  expect(on("k1abc n0call -05").kind).toBe("report");
  // Without a call of our own, the sender isn't checked.
  expect(on("K1ABC W9XYZ RR73", { mycall: "" }).kind).toBe("rr73");
});
