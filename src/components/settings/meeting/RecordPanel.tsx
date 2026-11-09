import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Circle } from "lucide-react";
import {
  commands,
  type MeetingKind,
  type MeetingStartOptions,
  type MeetingTopic,
} from "@/bindings";
import { notify } from "@/stores/noticeStore";
import { Button } from "../../ui/Button";
import { Input } from "../../ui/Input";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { ShortcutInput } from "../ShortcutInput";
import { formatElapsed } from "./elapsed";
import { KindPicker } from "./KindPicker";
import { TopicSelect } from "./TopicSelect";
import { useMeetingState } from "./useMeetings";

const TIMER_TICK_MS = 1000;

/** Milliseconds since `startedAtMs`, ticking once a second. */
const useElapsed = (startedAtMs: number | null): number => {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (startedAtMs === null) return;
    setNow(Date.now());
    const timer = window.setInterval(() => setNow(Date.now()), TIMER_TICK_MS);
    return () => window.clearInterval(timer);
  }, [startedAtMs]);
  return startedAtMs === null ? 0 : now - startedAtMs;
};

interface RecordPanelProps {
  topics: MeetingTopic[];
  onTopicsChanged: () => void;
}

export const RecordPanel: React.FC<RecordPanelProps> = ({
  topics,
  onTopicsChanged,
}) => {
  const { t } = useTranslation();
  const { recording, started_at_ms } = useMeetingState();
  const elapsed = useElapsed(started_at_ms);
  const [pending, setPending] = useState<"starting" | "stopping" | null>(null);
  // The last meeting went to History; shown until the next one starts.
  const [saved, setSaved] = useState(false);
  const [kind, setKind] = useState<MeetingKind>("online");
  const [topicId, setTopicId] = useState<number | null>(null);
  const [title, setTitle] = useState("");

  // Start the way the last meeting did, as the shortcut does.
  useEffect(() => {
    commands.getLastMeetingOptions().then((result) => {
      if (result.status !== "ok") return;
      setKind(result.data.kind);
      setTopicId(result.data.topic_id);
    });
  }, []);

  // A deleted topic is no longer offered.
  useEffect(() => {
    if (topicId !== null && !topics.some((topic) => topic.id === topicId)) {
      setTopicId(null);
    }
  }, [topics, topicId]);

  const start = async () => {
    setSaved(false);
    setPending("starting");
    const options: MeetingStartOptions = { kind, topic_id: topicId, title };
    const result = await commands.startMeeting(options);
    setPending(null);
    if (result.status === "error") {
      notify({
        title: t("settings.meeting.startFailed", { error: result.error }),
      });
    }
  };

  const stop = async () => {
    setPending("stopping");
    const result = await commands.stopMeeting();
    setPending(null);
    if (result.status === "error") {
      notify({
        title: t("settings.meeting.stopFailed", { error: result.error }),
      });
    } else {
      setSaved(true);
      setTitle("");
    }
  };

  const buttonLabel =
    pending === "starting"
      ? t("settings.meeting.starting")
      : pending === "stopping"
        ? t("settings.meeting.stopping")
        : recording
          ? t("settings.meeting.stop")
          : t("settings.meeting.start");

  return (
    <SettingsGroup
      title={t("settings.meeting.title")}
      description={t("settings.meeting.description")}
    >
      <div className="px-4 py-3 space-y-3">
        <KindPicker value={kind} onChange={setKind} disabled={recording} />
        <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
          <Input
            value={title}
            onChange={(event) => setTitle(event.target.value)}
            placeholder={t("settings.meeting.titlePlaceholder")}
            aria-label={t("settings.meeting.fields.title")}
            disabled={recording}
          />
          <TopicSelect
            topics={topics}
            value={topicId}
            onChange={setTopicId}
            onCreated={onTopicsChanged}
            disabled={recording}
          />
        </div>
        <p className="text-xs text-ink-3">
          {kind === "online"
            ? t("settings.meeting.kindOnlineHint")
            : t("settings.meeting.kindOnSiteHint")}
        </p>
      </div>
      <div className="px-4 py-3 flex items-center justify-between gap-4">
        <div className="flex items-center gap-2 text-sm" aria-live="polite">
          <Circle
            className={
              recording ? "w-3 h-3 fill-error text-error" : "w-3 h-3 text-ink-3"
            }
          />
          <span>
            {recording
              ? t("settings.meeting.recording")
              : saved
                ? t("settings.meeting.saved")
                : t("settings.meeting.idle")}
          </span>
          {recording && (
            <span className="font-mono tabular-nums text-ink-2">
              {formatElapsed(elapsed)}
            </span>
          )}
        </div>
        <Button
          variant={recording ? "danger" : "primary"}
          onClick={recording ? stop : start}
          disabled={pending !== null}
        >
          {buttonLabel}
        </Button>
      </div>
      <ShortcutInput shortcutId="meeting_toggle" grouped={true} />
    </SettingsGroup>
  );
};
