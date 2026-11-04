import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Circle, Headphones, ShieldCheck, Volume2 } from "lucide-react";
import { commands, events, type MeetingState } from "@/bindings";
import { notify } from "@/stores/noticeStore";
import { useOsType } from "@/hooks/useOsType";
import { Button } from "../../ui/Button";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { ShortcutInput } from "../ShortcutInput";
import { formatElapsed } from "./elapsed";

const TIMER_TICK_MS = 1000;

const IDLE: MeetingState = { recording: false, started_at_ms: null };

/** The backend's meeting state, kept current by its events. */
const useMeetingState = (): MeetingState => {
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

export const MeetingSettings: React.FC = () => {
  const { t } = useTranslation();
  const osType = useOsType();
  const { recording, started_at_ms } = useMeetingState();
  const elapsed = useElapsed(started_at_ms);
  const [pending, setPending] = useState<"starting" | "stopping" | null>(null);
  // The last meeting went to History; shown until the next one starts.
  const [saved, setSaved] = useState(false);

  const start = async () => {
    setSaved(false);
    setPending("starting");
    const result = await commands.startMeeting();
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

  const tips = [
    { icon: ShieldCheck, text: t("settings.meeting.consent") },
    { icon: Headphones, text: t("settings.meeting.headphones") },
    ...(osType === "macos"
      ? [{ icon: Volume2, text: t("settings.meeting.permission") }]
      : []),
  ];

  return (
    <div className="max-w-3xl w-full mx-auto space-y-6">
      <SettingsGroup
        title={t("settings.meeting.title")}
        description={t("settings.meeting.description")}
      >
        <div className="px-4 py-3 flex items-center justify-between gap-4">
          <div className="flex items-center gap-2 text-sm" aria-live="polite">
            <Circle
              className={
                recording
                  ? "w-3 h-3 fill-error text-error"
                  : "w-3 h-3 text-ink-3"
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
      <SettingsGroup title={t("settings.meeting.tipsTitle")}>
        {tips.map(({ icon: Icon, text }) => (
          <div key={text} className="px-4 py-3 flex items-start gap-3 text-sm">
            <Icon className="w-4 h-4 mt-0.5 shrink-0 text-ink-2" />
            <p className="text-ink-2">{text}</p>
          </div>
        ))}
      </SettingsGroup>
    </div>
  );
};
