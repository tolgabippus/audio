import { useCallback, useEffect, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { readFile } from "@tauri-apps/plugin-fs";
import {
  commands,
  events,
  type Meeting,
  type MeetingState,
  type MeetingTopic,
} from "@/bindings";
import type { OSType } from "@/lib/utils/keyboard";

const IDLE: MeetingState = { recording: false, started_at_ms: null };

/** The backend's meeting state, kept current by its events. */
export const useMeetingState = (): MeetingState => {
  const [state, setState] = useState<MeetingState>(IDLE);
  useEffect(() => {
    commands.getMeetingState().then(setState);
    const unlisten = events.meetingState.listen(({ payload }) =>
      setState(payload),
    );
    return () => {
      unlisten.then((off) => off());
    };
  }, []);
  return state;
};

export interface MeetingsData {
  meetings: Meeting[];
  topics: MeetingTopic[];
  loading: boolean;
  /** Why loading failed, if it did. */
  error: string | null;
  reload: () => Promise<void>;
}

/**
 * Every meeting and topic. Reloads when History changes (a meeting added,
 * transcribed, renamed or deleted) and when a recording starts or stops.
 */
export const useMeetings = (): MeetingsData => {
  const [meetings, setMeetings] = useState<Meeting[]>([]);
  const [topics, setTopics] = useState<MeetingTopic[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const reload = useCallback(async () => {
    const [meetingsResult, topicsResult] = await Promise.all([
      commands.listMeetings(),
      commands.listMeetingTopics(),
    ]);
    setLoading(false);
    if (meetingsResult.status === "error") {
      setError(meetingsResult.error);
      return;
    }
    if (topicsResult.status === "error") {
      setError(topicsResult.error);
      return;
    }
    setError(null);
    setMeetings(meetingsResult.data);
    setTopics(topicsResult.data);
  }, []);

  useEffect(() => {
    reload();
    const history = events.historyUpdatePayload.listen(() => reload());
    const meetingState = events.meetingState.listen(() => reload());
    return () => {
      history.then((off) => off());
      meetingState.then((off) => off());
    };
  }, [reload]);

  return { meetings, topics, loading, error, reload };
};

/** A playable URL for a recording in History. */
export const recordingUrl = async (
  fileName: string,
  osType: OSType,
): Promise<string | null> => {
  const result = await commands.getAudioFilePath(fileName);
  if (result.status !== "ok") return null;
  // Linux's webview can't stream the asset protocol.
  if (osType === "linux") {
    const data = await readFile(result.data);
    return URL.createObjectURL(new Blob([data], { type: "audio/wav" }));
  }
  return convertFileSrc(result.data, "asset");
};
