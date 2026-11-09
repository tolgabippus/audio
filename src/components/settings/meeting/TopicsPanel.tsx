import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { Check, Pencil, Plus, Trash2, X } from "lucide-react";
import { commands, type Meeting, type MeetingTopic } from "@/bindings";
import { notify } from "@/stores/noticeStore";
import { Button } from "../../ui/Button";
import { Input } from "../../ui/Input";
import { SettingsGroup } from "../../ui/SettingsGroup";

interface TopicRowProps {
  topic: MeetingTopic;
  count: number;
  onChanged: () => void;
}

const TopicRow: React.FC<TopicRowProps> = ({ topic, count, onChanged }) => {
  const { t } = useTranslation();
  const [editing, setEditing] = useState(false);
  const [name, setName] = useState(topic.name);
  const [confirmingDelete, setConfirmingDelete] = useState(false);

  const rename = async () => {
    const result = await commands.renameMeetingTopic(topic.id, name);
    if (result.status === "error") {
      notify({
        title: t("settings.meeting.topics.renameFailed", {
          error: result.error,
        }),
      });
      return;
    }
    setEditing(false);
    onChanged();
  };

  const remove = async () => {
    const result = await commands.deleteMeetingTopic(topic.id);
    if (result.status === "error") {
      notify({
        title: t("settings.meeting.topics.deleteFailed", {
          error: result.error,
        }),
      });
      return;
    }
    onChanged();
  };

  if (editing) {
    return (
      <form
        className="px-4 py-2 flex items-center gap-2"
        onSubmit={(event) => {
          event.preventDefault();
          rename();
        }}
      >
        <Input
          value={name}
          onChange={(event) => setName(event.target.value)}
          aria-label={t("settings.meeting.topics.name")}
          className="flex-1"
          autoFocus
        />
        <Button
          type="submit"
          variant="ghost"
          size="sm"
          title={t("settings.meeting.topics.save")}
        >
          <Check className="w-4 h-4" />
        </Button>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          title={t("settings.meeting.topics.cancel")}
          onClick={() => {
            setName(topic.name);
            setEditing(false);
          }}
        >
          <X className="w-4 h-4" />
        </Button>
      </form>
    );
  }

  return (
    <div className="px-4 py-2 flex items-center gap-2 text-sm">
      <span className="flex-1 truncate">{topic.name}</span>
      <span className="text-xs text-ink-3">
        {t("settings.meeting.topics.count", { count })}
      </span>
      {confirmingDelete ? (
        <>
          <Button
            variant="ghost"
            size="sm"
            onClick={() => setConfirmingDelete(false)}
          >
            {t("settings.meeting.topics.cancel")}
          </Button>
          <Button variant="danger" size="sm" onClick={remove}>
            {t("settings.meeting.topics.confirmDelete")}
          </Button>
        </>
      ) : (
        <>
          <Button
            variant="ghost"
            size="sm"
            title={t("settings.meeting.topics.rename")}
            onClick={() => setEditing(true)}
          >
            <Pencil className="w-4 h-4" />
          </Button>
          <Button
            variant="danger-ghost"
            size="sm"
            title={t("settings.meeting.topics.delete")}
            onClick={() => setConfirmingDelete(true)}
          >
            <Trash2 className="w-4 h-4" />
          </Button>
        </>
      )}
    </div>
  );
};

interface TopicsPanelProps {
  topics: MeetingTopic[];
  meetings: Meeting[];
  onChanged: () => void;
}

export const TopicsPanel: React.FC<TopicsPanelProps> = ({
  topics,
  meetings,
  onChanged,
}) => {
  const { t } = useTranslation();
  const [name, setName] = useState("");

  const add = async () => {
    const result = await commands.createMeetingTopic(name);
    if (result.status === "error") {
      notify({
        title: t("settings.meeting.topics.createFailed", {
          error: result.error,
        }),
      });
      return;
    }
    setName("");
    onChanged();
  };

  const countOf = (topicId: number) =>
    meetings.filter((meeting) => meeting.topic_id === topicId).length;

  return (
    <SettingsGroup
      title={t("settings.meeting.topics.title")}
      description={t("settings.meeting.topics.description")}
    >
      {topics.map((topic) => (
        <TopicRow
          key={`${topic.id}-${topic.name}`}
          topic={topic}
          count={countOf(topic.id)}
          onChanged={onChanged}
        />
      ))}
      <form
        className="px-4 py-2 flex items-center gap-2"
        onSubmit={(event) => {
          event.preventDefault();
          add();
        }}
      >
        <Input
          value={name}
          onChange={(event) => setName(event.target.value)}
          placeholder={t("settings.meeting.topics.newPlaceholder")}
          aria-label={t("settings.meeting.topics.name")}
          className="flex-1"
        />
        <Button
          type="submit"
          variant="secondary"
          size="sm"
          disabled={!name.trim()}
        >
          <span className="flex items-center gap-1">
            <Plus className="w-4 h-4" />
            {t("settings.meeting.topics.add")}
          </span>
        </Button>
      </form>
    </SettingsGroup>
  );
};
