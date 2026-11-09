import React, { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { ArrowLeft, Copy, FileDown, RotateCcw, Trash2, X } from "lucide-react";
import {
  commands,
  type Meeting,
  type MeetingDetails,
  type MeetingTopic,
  type Turn,
} from "@/bindings";
import { useOsType } from "@/hooks/useOsType";
import { notify } from "@/stores/noticeStore";
import { useTranscriptionJobsStore } from "@/stores/transcriptionJobsStore";
import { Button } from "../../ui/Button";
import { Input } from "../../ui/Input";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { Textarea } from "../../ui/Textarea";
import { copyToClipboard } from "../history/clipboard";
import { KindPicker } from "./KindPicker";
import { durationMinutes, formatTurnTime } from "./meetings";
import { TopicSelect } from "./TopicSelect";
import { recordingUrl } from "./useMeetings";

const MS_PER_SECOND = 1000;

const detailsOf = (meeting: Meeting): MeetingDetails => ({
  title: meeting.title,
  topic_id: meeting.topic_id,
  kind: meeting.kind,
  participants: meeting.participants,
  notes: meeting.notes,
});

const sameDetails = (a: MeetingDetails, b: MeetingDetails) =>
  a.title === b.title &&
  a.topic_id === b.topic_id &&
  a.kind === b.kind &&
  a.participants === b.participants &&
  a.notes === b.notes;

/** The meeting's turns, read again whenever its transcript changes. */
const useTurns = (meeting: Meeting): Turn[] | null => {
  const [turns, setTurns] = useState<Turn[] | null>(null);
  useEffect(() => {
    let current = true;
    commands.getMeetingTurns(meeting.id).then((result) => {
      if (current) setTurns(result.status === "ok" ? result.data : null);
    });
    return () => {
      current = false;
    };
  }, [meeting.id, meeting.transcript]);
  return turns;
};

interface MeetingDetailProps {
  meeting: Meeting;
  topics: MeetingTopic[];
  canExport: boolean;
  onBack: () => void;
  onChanged: () => void;
}

export const MeetingDetail: React.FC<MeetingDetailProps> = ({
  meeting,
  topics,
  canExport,
  onBack,
  onChanged,
}) => {
  const { t, i18n } = useTranslation();
  const osType = useOsType();
  const job = useTranscriptionJobsStore((state) =>
    meeting.history_id === null ? undefined : state.jobs[meeting.history_id],
  );
  const turns = useTurns(meeting);
  const audioRef = useRef<HTMLAudioElement>(null);
  const [audioSrc, setAudioSrc] = useState<string | null>(null);
  const [details, setDetails] = useState(() => detailsOf(meeting));
  const [saving, setSaving] = useState(false);
  const [confirmingDelete, setConfirmingDelete] = useState(false);

  const saved = detailsOf(meeting);
  const dirty = !sameDetails(details, saved);

  // Take the saved details when another meeting opens.
  useEffect(() => {
    setDetails(detailsOf(meeting));
    setConfirmingDelete(false);
    // Only on a different meeting: an edit in progress survives a reload.
  }, [meeting.id]);

  useEffect(() => {
    if (meeting.history_id === null) return;
    let url: string | null = null;
    recordingUrl(meeting.file_name, osType)
      .then((src) => {
        url = src;
        setAudioSrc(src);
      })
      .catch((error) => console.error("Failed to load the recording:", error));
    return () => {
      if (url?.startsWith("blob:")) URL.revokeObjectURL(url);
    };
  }, [meeting.file_name, meeting.history_id, osType]);

  const update = <K extends keyof MeetingDetails>(
    key: K,
    value: MeetingDetails[K],
  ) => setDetails((prev) => ({ ...prev, [key]: value }));

  const save = async () => {
    setSaving(true);
    const result = await commands.updateMeeting(meeting.id, details);
    setSaving(false);
    if (result.status === "error") {
      notify({
        title: t("settings.meeting.detail.saveFailed", { error: result.error }),
      });
      return;
    }
    onChanged();
  };

  const playFrom = (startMs: number) => {
    const audio = audioRef.current;
    if (!audio) return;
    audio.currentTime = startMs / MS_PER_SECOND;
    audio.play().catch((error) => console.error("Failed to play:", error));
  };

  const retranscribe = () => {
    if (meeting.history_id !== null) {
      commands.retryHistoryEntryTranscription(meeting.history_id);
    }
  };

  const cancelTranscription = () => {
    if (meeting.history_id !== null) {
      commands.cancelHistoryEntryTranscription(meeting.history_id);
    }
  };

  const exportNote = async () => {
    const result = await commands.exportMeeting(meeting.id);
    notify({
      title:
        result.status === "ok"
          ? t("settings.meeting.export.done")
          : t("settings.meeting.export.failed", { error: result.error }),
    });
  };

  const copyTranscript = async () => {
    const copied = await copyToClipboard(meeting.transcript);
    notify({
      title: copied
        ? t("settings.meeting.detail.copied")
        : t("settings.meeting.detail.copyFailed"),
    });
  };

  const remove = async () => {
    if (meeting.history_id === null) return;
    const result = await commands.deleteHistoryEntry(meeting.history_id);
    if (result.status === "error") {
      notify({
        title: t("settings.meeting.detail.deleteFailed", {
          error: result.error,
        }),
      });
      return;
    }
    onBack();
  };

  const started = new Intl.DateTimeFormat(i18n.language, {
    dateStyle: "full",
    timeStyle: "short",
  }).format(new Date(meeting.started_at_ms));
  const busy = job !== undefined;
  const hasEntry = meeting.history_id !== null;

  let transcript: React.ReactNode;
  if (job) {
    transcript = (
      <p className="px-4 py-3 text-sm text-ink-2">
        {job.state === "running"
          ? t("settings.meeting.list.transcribing", { percent: job.percent })
          : t("settings.meeting.list.queued")}
      </p>
    );
  } else if (turns && turns.length > 0) {
    transcript = (
      <div className="px-4 py-3 space-y-3">
        {turns.map((turn) => {
          const you = turn.speaker === "you";
          return (
            <div key={`${turn.speaker}-${turn.start_ms}`} className="text-sm">
              <div className="flex items-center gap-2 mb-0.5">
                <button
                  type="button"
                  onClick={() => playFrom(turn.start_ms)}
                  disabled={!audioSrc}
                  title={t("settings.meeting.detail.playFrom")}
                  className="font-mono text-xs text-ink-3 hover:text-logo-primary disabled:hover:text-ink-3 tabular-nums"
                >
                  {formatTurnTime(turn.start_ms)}
                </button>
                <span
                  className={`text-xs font-semibold ${you ? "text-logo-primary" : "text-plum"}`}
                >
                  {you
                    ? t("tray.meetingSpeakerYou")
                    : t("tray.meetingSpeakerOthers")}
                </span>
              </div>
              <p
                className={`ps-3 border-s-2 select-text ${you ? "border-logo-primary" : "border-plum"}`}
              >
                {turn.text}
              </p>
            </div>
          );
        })}
      </div>
    );
  } else if (meeting.transcript.trim()) {
    transcript = (
      <p className="px-4 py-3 text-sm whitespace-pre-wrap select-text">
        {meeting.transcript}
      </p>
    );
  } else {
    transcript = (
      <p className="px-4 py-3 text-sm text-ink-2">
        {hasEntry
          ? t("settings.meeting.list.noText")
          : t("settings.meeting.list.noEntry")}
      </p>
    );
  }

  return (
    <div className="space-y-6">
      <div className="flex items-center gap-2">
        <Button variant="ghost" size="sm" onClick={onBack}>
          <span className="flex items-center gap-1">
            <ArrowLeft className="w-4 h-4" />
            {t("settings.meeting.detail.back")}
          </span>
        </Button>
      </div>
      <div className="px-4">
        <h1 className="text-lg font-semibold">
          {meeting.title.trim() || t("settings.meeting.untitled")}
        </h1>
        <p className="text-sm text-ink-3">
          {started}
          {" · "}
          {t("settings.meeting.minutes", {
            count: durationMinutes(meeting.duration_ms),
          })}
        </p>
      </div>

      <SettingsGroup title={t("settings.meeting.detail.details")}>
        <div className="px-4 py-3 space-y-3">
          <label className="block space-y-1">
            <span className="text-xs text-ink-3">
              {t("settings.meeting.fields.title")}
            </span>
            <Input
              value={details.title}
              onChange={(event) => update("title", event.target.value)}
              placeholder={t("settings.meeting.titlePlaceholder")}
              className="w-full"
            />
          </label>
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
            <div className="space-y-1">
              <span className="block text-xs text-ink-3">
                {t("settings.meeting.fields.topic")}
              </span>
              <TopicSelect
                topics={topics}
                value={details.topic_id}
                onChange={(topicId) => update("topic_id", topicId)}
                onCreated={onChanged}
              />
            </div>
            <div className="space-y-1">
              <span className="block text-xs text-ink-3">
                {t("settings.meeting.fields.kind")}
              </span>
              <KindPicker
                value={details.kind}
                onChange={(kind) => update("kind", kind)}
              />
            </div>
          </div>
          <label className="block space-y-1">
            <span className="text-xs text-ink-3">
              {t("settings.meeting.fields.participants")}
            </span>
            <Input
              value={details.participants}
              onChange={(event) => update("participants", event.target.value)}
              placeholder={t("settings.meeting.fields.participantsPlaceholder")}
              className="w-full"
            />
          </label>
          <label className="block space-y-1">
            <span className="text-xs text-ink-3">
              {t("settings.meeting.fields.notes")}
            </span>
            <Textarea
              value={details.notes}
              onChange={(event) => update("notes", event.target.value)}
              placeholder={t("settings.meeting.fields.notesPlaceholder")}
              className="w-full"
            />
          </label>
          <div className="flex justify-end gap-2">
            {dirty && (
              <Button
                variant="ghost"
                size="sm"
                onClick={() => setDetails(saved)}
                disabled={saving}
              >
                {t("settings.meeting.detail.discard")}
              </Button>
            )}
            <Button size="sm" onClick={save} disabled={!dirty || saving}>
              {saving
                ? t("settings.meeting.detail.saving")
                : t("settings.meeting.detail.save")}
            </Button>
          </div>
        </div>
      </SettingsGroup>

      {hasEntry && (
        <SettingsGroup title={t("settings.meeting.detail.recording")}>
          <div className="px-4 py-3">
            {audioSrc ? (
              <audio
                ref={audioRef}
                src={audioSrc}
                controls
                preload="metadata"
                className="w-full"
              />
            ) : (
              <p className="text-sm text-ink-2">
                {t("settings.meeting.detail.loadingAudio")}
              </p>
            )}
          </div>
        </SettingsGroup>
      )}

      <SettingsGroup
        title={t("settings.meeting.detail.transcript")}
        description={
          turns && turns.length > 0
            ? t("settings.meeting.detail.transcriptHint")
            : undefined
        }
      >
        {transcript}
      </SettingsGroup>

      <div className="flex flex-wrap gap-2 px-4">
        {busy ? (
          <Button variant="secondary" size="sm" onClick={cancelTranscription}>
            <span className="flex items-center gap-1">
              <X className="w-4 h-4" />
              {t("settings.meeting.detail.cancelTranscription")}
            </span>
          </Button>
        ) : (
          <Button
            variant="secondary"
            size="sm"
            onClick={retranscribe}
            disabled={!hasEntry}
          >
            <span className="flex items-center gap-1">
              <RotateCcw className="w-4 h-4" />
              {t("settings.meeting.detail.retranscribe")}
            </span>
          </Button>
        )}
        <Button
          variant="secondary"
          size="sm"
          onClick={copyTranscript}
          disabled={!meeting.transcript.trim()}
        >
          <span className="flex items-center gap-1">
            <Copy className="w-4 h-4" />
            {t("settings.meeting.detail.copy")}
          </span>
        </Button>
        <Button
          variant="secondary"
          size="sm"
          onClick={exportNote}
          disabled={!canExport}
          title={canExport ? undefined : t("settings.meeting.export.noFolder")}
        >
          <span className="flex items-center gap-1">
            <FileDown className="w-4 h-4" />
            {t("settings.meeting.detail.export")}
          </span>
        </Button>
        <div className="flex-1" />
        {confirmingDelete ? (
          <>
            <Button
              variant="ghost"
              size="sm"
              onClick={() => setConfirmingDelete(false)}
            >
              {t("settings.meeting.detail.keep")}
            </Button>
            <Button variant="danger" size="sm" onClick={remove}>
              {t("settings.meeting.detail.confirmDelete")}
            </Button>
          </>
        ) : (
          <Button
            variant="danger-ghost"
            size="sm"
            onClick={() => setConfirmingDelete(true)}
            disabled={!hasEntry || busy}
          >
            <span className="flex items-center gap-1">
              <Trash2 className="w-4 h-4" />
              {t("settings.meeting.detail.delete")}
            </span>
          </Button>
        )}
      </div>
    </div>
  );
};
