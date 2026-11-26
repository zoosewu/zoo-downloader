// Basic info tab: URL, the page it came from, file path, size, timestamps and the user's note.
import React, { useEffect, useState } from 'react';
import { Box, Button, Chip, Link, TextField } from '@mui/material';
import { Edit as EditIcon } from '@mui/icons-material';
import { useTranslation } from 'react-i18next';
import { backend, type Task } from '../backend';
import { formatBytes } from '../utils/downloadCalculations';
import { formatAbsoluteTime } from '../utils/formatters';
import PropertyList from './PropertyList';
import EditTaskDialog, { editable } from './EditTaskDialog';
import { useCategories } from '../hooks/useCategories';
import { useSiteModules } from '../hooks/useSiteModules';

const BasicInfoTab: React.FC<{ task: Task }> = ({ task }) => {
  const { t } = useTranslation();
  const [editing, setEditing] = useState(false);
  const { categories } = useCategories();
  const inCategories = categories.filter((c) => task.tags.includes(c.id));
  const moduleName = useModuleName(task.module, task.url);

  return (
    <Box sx={{ p: 1.5, display: 'flex', gap: 1, alignItems: 'flex-start' }}>
      <Box sx={{ flex: 1, minWidth: 0 }}>
        <PropertyList
          items={[
            { label: t('downloads.details.url'), value: <Link href={task.url} target="_blank" rel="noreferrer" underline="hover">{task.url}</Link> },
            task.pageUrl && {
              label: t('downloads.details.pageUrl'),
              value: <Link href={task.pageUrl} target="_blank" rel="noreferrer" underline="hover">{task.pageUrl}</Link>,
            },
            {
              label: t('downloads.details.module'),
              value: moduleName ?? task.module ?? t('downloads.details.directDownload'),
            },
            { label: t('downloads.details.filePath'), value: task.finalPath ?? `${task.dir}\\${task.name}`, mono: true },
            {
              label: t('downloads.details.fileSize'),
              value: task.total !== null ? `${formatBytes(task.total)} (${task.total.toLocaleString()} bytes)` : '--',
            },
            { label: t('downloads.details.createdAt'), value: formatAbsoluteTime(task.createdAt) },
            task.startedAt && { label: t('downloads.details.startedAt'), value: formatAbsoluteTime(task.startedAt) },
            task.state === 'completed' &&
              task.finishedAt && { label: t('downloads.details.completedAt'), value: formatAbsoluteTime(task.finishedAt) },
            // Keyed by task: switching tasks starts from that task's note.
            { label: t('downloads.details.note'), value: <NoteField key={task.id} task={task} /> },
            inCategories.length > 0 && {
              label: t('sidebar.categories'),
              value: (
                <Box sx={{ display: 'flex', gap: 0.5, flexWrap: 'wrap' }}>
                  {inCategories.map((c) => (
                    <Chip key={c.id} label={c.name} variant="outlined" />
                  ))}
                </Box>
              ),
            },
          ]}
        />
      </Box>
      {editable(task) && (
        <Button color="inherit" startIcon={<EditIcon fontSize="small" />} onClick={() => setEditing(true)}>
          {t('downloads.edit.title')}
        </Button>
      )}
      <EditTaskDialog task={editing ? task : null} onClose={() => setEditing(false)} />
    </Box>
  );
};

/** The task's note, saved when the field loses focus (or on Ctrl+Enter). */
function NoteField({ task }: { task: Task }) {
  const { t } = useTranslation();
  const [text, setText] = useState(task.note);
  const [error, setError] = useState<string | null>(null);
  const save = () => {
    if (text.trim() === task.note) return;
    backend.downloads
      .setNote(task.id, text)
      .then((saved) => {
        setText(saved.note);
        setError(null);
      })
      .catch((e) => setError(e.message));
  };
  return (
    <TextField
      value={text}
      onChange={(e) => setText(e.target.value)}
      onBlur={save}
      onKeyDown={(e) => {
        if (e.key === 'Enter' && e.ctrlKey) save();
      }}
      placeholder={t('downloads.details.notePlaceholder')}
      // A red outline with the reason on hover: a message below would push the panel around.
      error={error !== null}
      title={error ?? undefined}
      multiline
      maxRows={6}
      fullWidth
      size="small"
    />
  );
}

/**
 * Name of the module that resolved the task (`key`, `id@marketplace`). Tasks from before the
 * module was recorded have none: the module that handles the URL now is shown instead.
 */
function useModuleName(key: string | null, url: string): string | null {
  const { installed } = useSiteModules();
  const [byUrl, setByUrl] = useState<string | null>(null);
  useEffect(() => {
    setByUrl(null);
    if (key) return;
    let current = true;
    backend.urls
      .inspect(url)
      .then((info) => current && setByUrl(info.module?.name ?? null))
      .catch(() => {});
    return () => {
      current = false;
    };
  }, [key, url]);
  if (!key) return byUrl;
  // Recorded under a marketplace's old name (`baidu@official` before it was `zoo`): the one
  // installed module with that id.
  const id = key.split('@')[0];
  const byId = installed.filter((m) => m.id === id);
  return (installed.find((m) => m.key === key) ?? (byId.length === 1 ? byId[0] : undefined))?.name ?? null;
}

export default BasicInfoTab;
