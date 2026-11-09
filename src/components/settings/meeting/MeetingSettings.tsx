import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Headphones, ShieldCheck, Volume2 } from "lucide-react";
import { commands } from "@/bindings";
import { useOsType } from "@/hooks/useOsType";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { ExportPanel } from "./ExportPanel";
import { MeetingDetail } from "./MeetingDetail";
import { MeetingList } from "./MeetingList";
import { RecordPanel } from "./RecordPanel";
import { TopicsPanel } from "./TopicsPanel";
import { useMeetings } from "./useMeetings";

/** The folder meetings are exported to, as the backend has it. */
const useExportDir = () => {
  const [dir, setDir] = useState<string | null>(null);
  useEffect(() => {
    commands.getMeetingExportDir().then((result) => {
      if (result.status === "ok") setDir(result.data);
    });
  }, []);
  return [dir, setDir] as const;
};

export const MeetingSettings: React.FC = () => {
  const { t } = useTranslation();
  const osType = useOsType();
  const { meetings, topics, loading, error, reload } = useMeetings();
  const [exportDir, setExportDir] = useExportDir();
  const [openId, setOpenId] = useState<string | null>(null);

  const open =
    openId === null ? undefined : meetings.find((m) => m.id === openId);

  // A meeting deleted elsewhere closes.
  useEffect(() => {
    if (openId !== null && !loading && !open) setOpenId(null);
  }, [openId, open, loading]);

  if (open) {
    return (
      <div className="max-w-3xl w-full mx-auto">
        <MeetingDetail
          meeting={open}
          topics={topics}
          canExport={exportDir !== null}
          onBack={() => setOpenId(null)}
          onChanged={reload}
        />
      </div>
    );
  }

  const tips = [
    { icon: ShieldCheck, text: t("settings.meeting.consent") },
    { icon: Headphones, text: t("settings.meeting.headphones") },
    ...(osType === "macos"
      ? [{ icon: Volume2, text: t("settings.meeting.permission") }]
      : []),
  ];

  return (
    <div className="max-w-3xl w-full mx-auto space-y-6">
      <RecordPanel topics={topics} onTopicsChanged={reload} />
      {error && (
        <p className="px-4 text-sm text-error">
          {t("settings.meeting.list.loadFailed", { error })}
        </p>
      )}
      <MeetingList
        meetings={meetings}
        topics={topics}
        loading={loading}
        onOpen={setOpenId}
      />
      <TopicsPanel topics={topics} meetings={meetings} onChanged={reload} />
      <ExportPanel dir={exportDir} onChanged={setExportDir} />
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
