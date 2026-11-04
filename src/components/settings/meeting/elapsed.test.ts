import assert from "node:assert/strict";
import { formatElapsed } from "./elapsed";

assert.equal(formatElapsed(0), "0:00");
assert.equal(formatElapsed(999), "0:00");
assert.equal(formatElapsed(65_000), "1:05");
assert.equal(formatElapsed(59 * 60_000 + 59_999), "59:59");
assert.equal(formatElapsed(3_600_000), "1:00:00");
assert.equal(formatElapsed(2 * 3_600_000 + 3 * 60_000 + 4_000), "2:03:04");
// A clock that went backwards shows zero, not a negative time.
assert.equal(formatElapsed(-5_000), "0:00");

console.log("elapsed: all assertions passed");
