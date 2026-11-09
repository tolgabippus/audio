import React from "react";
import { useTranslation } from "react-i18next";
import { commands, type MeetingTopic } from "@/bindings";
import { notify } from "@/stores/noticeStore";
import { Select } from "../../ui/Select";

interface TopicSelectProps {
  topics: MeetingTopic[];
  value: number | null;
  onChange: (topicId: number | null) => void;
  /** A topic was typed in and created; reload the topics. */
  onCreated: () => void;
  disabled?: boolean;
}

/** Picks a meeting's topic, or creates one by typing its name. */
export const TopicSelect: React.FC<TopicSelectProps> = ({
  topics,
  value,
  onChange,
  onCreated,
  disabled = false,
}) => {
  const { t } = useTranslation();

  const create = async (name: string) => {
    const result = await commands.createMeetingTopic(name);
    if (result.status === "error") {
      notify({
        title: t("settings.meeting.topics.createFailed", {
          error: result.error,
        }),
      });
      return;
    }
    onCreated();
    onChange(result.data.id);
  };

  return (
    <Select
      value={value === null ? null : String(value)}
      options={topics.map((topic) => ({
        value: String(topic.id),
        label: topic.name,
      }))}
      placeholder={t("settings.meeting.noTopic")}
      onChange={(selected) =>
        onChange(selected === null ? null : Number(selected))
      }
      disabled={disabled}
      isCreatable={true}
      onCreateOption={create}
      formatCreateLabel={(input) =>
        t("settings.meeting.topics.createOption", { name: input })
      }
    />
  );
};
