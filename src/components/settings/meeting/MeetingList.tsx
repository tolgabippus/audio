import React, { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { ChevronRight, MonitorSpeaker, Search, Users } from "lucide-react";
import type { Meeting, MeetingTopic } from "@/bindings";
import { useTranscriptionJobsStore } from "@/stores/transcriptionJobsStore";
import { Input } from "../../ui/Input";
import { Select } from "../../ui/Select";
import { SettingsGroup } from "../../ui/SettingsGroup";
import {
  ALL_TOPICS,
  NO_TOPIC,
  durationMinutes,
  filterMeetings,
} from "./meetings";

interface MeetingListProps {
  meetings: Meeting[];
  topics: MeetingTopic[];
  loading: boolean;
  onOpen: (id: string) => void;
}

export const MeetingList: React.FC<MeetingListProps> = ({
  meetings,
  topics,
  loading,
  onOpen,
}) => {
  const { t, i18n } = useTranslation();
  const jobs = useTranscriptionJobsStore((state) => state.jobs);
  const [query, setQuery] = useState("");
  const [topic, setTopic] = useState(ALL_TOPICS);

  const shown = useMemo(
    () => filterMeetings(meetings, { query, topic }),
    [meetings, query, topic],
  );
  const topicNames = useMemo(
    () => new Map(topics.map((tp) => [tp.id, tp.name])),
    [topics],
  );
  const dateFormat = useMemo(
    () =>
      new Intl.DateTimeFormat(i18n.language, {
        dateStyle: "medium",
        timeStyle: "short",
      }),
    [i18n.language],
  );

  const topicOptions = [
    { value: ALL_TOPICS, label: t("settings.meeting.list.allTopics") },
    { value: NO_TOPIC, label: t("settings.meeting.noTopic") },
    ...topics.map((tp) => ({ value: String(tp.id), label: tp.name })),
  ];

  const status = (meeting: Meeting): string | null => {
    const job =
      meeting.history_id === null ? undefined : jobs[meeting.history_id];
    if (job?.state === "running")
      return t("settings.meeting.list.transcribing", { percent: job.percent });
    if (job?.state === "queued") return t("settings.meeting.list.queued");
    if (meeting.history_id === null) return t("settings.meeting.list.noEntry");
    if (meeting.transcript.trim() === "")
      return t("settings.meeting.list.noText");
    return null;
  };

  let content: React.ReactNode;
  if (loading) {
    content = (
      <p className="px-4 py-3 text-sm text-center text-ink-2">
        {t("settings.meeting.list.loading")}
      </p>
    );
  } else if (meetings.length === 0) {
    content = (
      <p className="px-4 py-3 text-sm text-center text-ink-2">
        {t("settings.meeting.list.empty")}
      </p>
    );
  } else if (shown.length === 0) {
    content = (
      <p className="px-4 py-3 text-sm text-center text-ink-2">
        {t("settings.meeting.list.noMatch")}
      </p>
    );
  } else {
    content = shown.map((meeting) => {
      const KindIcon = meeting.kind === "online" ? MonitorSpeaker : Users;
      const topicName =
        meeting.topic_id === null ? null : topicNames.get(meeting.topic_id);
      const note = status(meeting);
      return (
        <button
          key={meeting.id}
          type="button"
          onClick={() => onOpen(meeting.id)}
          className="w-full px-4 py-3 flex items-center gap-3 text-start hover:bg-logo-primary/10 transition-colors"
        >
          <KindIcon className="w-4 h-4 shrink-0 text-ink-3" />
          <div className="min-w-0 flex-1">
            <p className="text-sm font-medium truncate">
              {meeting.title.trim() || t("settings.meeting.untitled")}
            </p>
            <p className="text-xs text-ink-3 truncate">
              {dateFormat.format(new Date(meeting.started_at_ms))}
              {" · "}
              {t("settings.meeting.minutes", {
                count: durationMinutes(meeting.duration_ms),
              })}
              {note && ` · ${note}`}
            </p>
          </div>
          {topicName && (
            <span className="shrink-0 max-w-[40%] truncate rounded-full bg-logo-primary/15 px-2 py-0.5 text-xs">
              {topicName}
            </span>
          )}
          <ChevronRight className="w-4 h-4 shrink-0 text-ink-3" />
        </button>
      );
    });
  }

  return (
    <SettingsGroup title={t("settings.meeting.list.title")}>
      <div className="px-4 py-3 grid grid-cols-1 sm:grid-cols-2 gap-3">
        <div className="relative">
          <Search className="w-4 h-4 absolute start-3 top-1/2 -translate-y-1/2 text-ink-3 pointer-events-none" />
          <Input
            type="search"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder={t("settings.meeting.list.search")}
            aria-label={t("settings.meeting.list.search")}
            className="w-full ps-9"
          />
        </div>
        <Select
          value={topic}
          options={topicOptions}
          isClearable={false}
          onChange={(value) => setTopic(value ?? ALL_TOPICS)}
        />
      </div>
      {content}
    </SettingsGroup>
  );
};
