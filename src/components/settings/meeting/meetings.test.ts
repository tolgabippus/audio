import assert from "node:assert/strict";
import type { Meeting } from "@/bindings";
import {
  ALL_TOPICS,
  NO_TOPIC,
  durationMinutes,
  filterMeetings,
  formatTurnTime,
} from "./meetings";

const meeting = (overrides: Partial<Meeting>): Meeting => ({
  id: "1",
  file_name: "handy-meeting-1.wav",
  history_id: 1,
  title: "",
  topic_id: null,
  kind: "online",
  started_at_ms: 0,
  duration_ms: 0,
  participants: "",
  notes: "",
  transcript: "",
  ...overrides,
});

const kickoff = meeting({ id: "a", title: "Kickoff Projekt X", topic_id: 1 });
const weekly = meeting({
  id: "b",
  participants: "Anna, Ben",
  transcript: "[00:00:01] Andere: Das Angebot geht raus.",
});
const all = [kickoff, weekly];
const ids = (list: Meeting[]) => list.map((m) => m.id);

assert.deepEqual(ids(filterMeetings(all, { query: "", topic: ALL_TOPICS })), [
  "a",
  "b",
]);
assert.deepEqual(ids(filterMeetings(all, { query: "", topic: "1" })), ["a"]);
assert.deepEqual(ids(filterMeetings(all, { query: "", topic: NO_TOPIC })), [
  "b",
]);
// Every word has to match, in any field, ignoring case.
assert.deepEqual(
  ids(filterMeetings(all, { query: "anna angebot", topic: ALL_TOPICS })),
  ["b"],
);
assert.deepEqual(
  ids(filterMeetings(all, { query: "kickoff anna", topic: ALL_TOPICS })),
  [],
);
assert.deepEqual(
  ids(filterMeetings(all, { query: "  PROJEKT ", topic: ALL_TOPICS })),
  ["a"],
);

assert.equal(durationMinutes(0), 1);
assert.equal(durationMinutes(89_000), 1);
assert.equal(durationMinutes(91_000), 2);

assert.equal(formatTurnTime(72_500), "00:01:12");
assert.equal(formatTurnTime(3_725_000), "01:02:05");
assert.equal(formatTurnTime(-1), "00:00:00");

console.log("meetings: all assertions passed");
