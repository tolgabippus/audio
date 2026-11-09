import type { Meeting } from "@/bindings";

const MS_PER_SECOND = 1000;
const SECONDS_PER_MINUTE = 60;
const SECONDS_PER_HOUR = 3600;
const MS_PER_MINUTE = MS_PER_SECOND * SECONDS_PER_MINUTE;

/** Shown for a meeting without a topic in the topic filter. */
export const NO_TOPIC = "none";
/** The topic filter's "every meeting". */
export const ALL_TOPICS = "all";

export interface MeetingFilter {
  query: string;
  /** A topic id, {@link ALL_TOPICS} or {@link NO_TOPIC}. */
  topic: string;
}

const matchesTopic = (meeting: Meeting, topic: string) => {
  if (topic === ALL_TOPICS) return true;
  if (topic === NO_TOPIC) return meeting.topic_id === null;
  return String(meeting.topic_id) === topic;
};

/** Every word of `query` appears in the title, people, notes or transcript. */
const matchesQuery = (meeting: Meeting, query: string) => {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (words.length === 0) return true;
  const haystack = [
    meeting.title,
    meeting.participants,
    meeting.notes,
    meeting.transcript,
  ]
    .join("\n")
    .toLowerCase();
  return words.every((word) => haystack.includes(word));
};

export const filterMeetings = (
  meetings: Meeting[],
  { query, topic }: MeetingFilter,
): Meeting[] =>
  meetings.filter((m) => matchesTopic(m, topic) && matchesQuery(m, query));

/** A meeting's length in whole minutes, at least one. */
export const durationMinutes = (durationMs: number): number =>
  Math.max(1, Math.round(durationMs / MS_PER_MINUTE));

/** A turn's start in the recording, `hh:mm:ss` like the transcript. */
export const formatTurnTime = (startMs: number): string => {
  const total = Math.max(0, Math.floor(startMs / MS_PER_SECOND));
  const pad = (n: number) => n.toString().padStart(2, "0");
  const hours = Math.floor(total / SECONDS_PER_HOUR);
  const minutes = Math.floor((total % SECONDS_PER_HOUR) / SECONDS_PER_MINUTE);
  return `${pad(hours)}:${pad(minutes)}:${pad(total % SECONDS_PER_MINUTE)}`;
};
