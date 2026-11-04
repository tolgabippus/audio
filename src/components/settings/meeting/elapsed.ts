const MS_PER_SECOND = 1000;
const SECONDS_PER_MINUTE = 60;
const SECONDS_PER_HOUR = 3600;

const twoDigits = (value: number) => value.toString().padStart(2, "0");

/** A recording's running time: `m:ss`, or `h:mm:ss` from an hour on. */
export const formatElapsed = (elapsedMs: number): string => {
  const total = Math.max(0, Math.floor(elapsedMs / MS_PER_SECOND));
  const hours = Math.floor(total / SECONDS_PER_HOUR);
  const minutes = Math.floor((total % SECONDS_PER_HOUR) / SECONDS_PER_MINUTE);
  const seconds = total % SECONDS_PER_MINUTE;
  return hours > 0
    ? `${hours}:${twoDigits(minutes)}:${twoDigits(seconds)}`
    : `${minutes}:${twoDigits(seconds)}`;
};
