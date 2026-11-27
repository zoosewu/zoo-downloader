// Settings → About, the last tab (zoo-release docs/UI.md §2): the version and its update status,
// "check for updates", automatic checks, links, and the author's other projects.
import { useEffect, useState } from 'react';
import { Box, Button, Card, CardActionArea, CardContent, CardMedia, Checkbox, CircularProgress, FormControlLabel, Link, Paper, Stack, Typography } from '@mui/material';
import { useTranslation } from 'react-i18next';
import { useAtomValue } from 'jotai';
import { settingsAtom } from '../stores/settingsStore';
import { updateStateAtom, useAppUpdate, useAppVersion } from '../hooks/useAppUpdate';
import { useUpdateSettings } from '../hooks/useUpdateSettings';
import { REPO_URL, formatCheckedAt } from '../utils/appUpdate';
import { useUpdateContent } from './UpdateBanner';

const LICENSE = 'AGPL-3.0';
const PROJECTS_URL = 'https://zoosewu.github.io/apex-code/api/v1/projects.json';

export default function AboutSettings() {
  const { t } = useTranslation();
  const version = useAppVersion();
  const settings = useAtomValue(settingsAtom);
  const update = useUpdateSettings();
  const state = useAtomValue(updateStateAtom);
  const { checkNow } = useAppUpdate();
  const busy = state.kind === 'checking' || state.kind === 'downloading' || state.kind === 'ready' || state.kind === 'restarting';

  return (
    <Box>
      <Paper variant="outlined" sx={{ p: 2.5, mb: 3 }}>
        <Box sx={{ display: 'flex', alignItems: 'flex-start', gap: 2 }}>
          <Box sx={{ flex: 1, minWidth: 0 }}>
            <Typography variant="h6" component="h2">
              Zoo Downloader{' '}
              <Typography component="span" variant="body1" color="text.secondary">
                {version && `v${version}`}
              </Typography>
            </Typography>
            <Typography variant="body2" color="text.secondary">
              {t('about.tagline')}
            </Typography>
          </Box>
          <Button
            variant="outlined"
            disabled={busy}
            onClick={() => checkNow(true)}
            // The spinner's place is kept, so the button keeps its width while checking.
            startIcon={<Box sx={{ width: 12, height: 12, display: 'flex' }}>{state.kind === 'checking' && <CircularProgress size={12} />}</Box>}
          >
            {t('update.check')}
          </Button>
        </Box>

        <Box sx={{ mt: 2, minHeight: 32, display: 'flex', alignItems: 'center' }}>
          <Status />
        </Box>
        <FormControlLabel
          control={<Checkbox checked={settings.checkAppUpdates} onChange={(e) => update({ checkAppUpdates: e.target.checked })} />}
          label={<Typography variant="body2">{t('update.auto')}</Typography>}
        />

        <Stack direction="row" spacing={1} divider={<Typography color="text.disabled">·</Typography>} sx={{ mt: 1, flexWrap: 'wrap' }}>
          <Link href={`${REPO_URL}/releases`} target="_blank" rel="noreferrer" variant="body2">
            {t('update.notes')}
          </Link>
          <Link href={REPO_URL} target="_blank" rel="noreferrer" variant="body2">
            {t('about.source')}
          </Link>
          <Link href={`${REPO_URL}/issues`} target="_blank" rel="noreferrer" variant="body2">
            {t('about.issues')}
          </Link>
          <Link href={`${REPO_URL}/blob/main/LICENSE`} target="_blank" rel="noreferrer" variant="body2">
            {t('about.license', { license: LICENSE })}
          </Link>
        </Stack>
      </Paper>

      <OtherProjects />
    </Box>
  );
}

/** The update status line. */
function Status() {
  const { t, i18n } = useTranslation();
  const state = useAtomValue(updateStateAtom);
  const lastCheck = useAtomValue(settingsAtom).updateLastCheck;
  const { checkNow } = useAppUpdate();
  const content = useUpdateContent(state, { withSkip: false });

  if (content) {
    return (
      <Box sx={{ display: 'flex', alignItems: 'center', gap: 1, flexWrap: 'wrap' }}>
        <Typography variant="body2" component="div" color={state.kind === 'failed' ? 'error' : 'text.primary'}>
          {content.message}
        </Typography>
        {content.actions}
      </Box>
    );
  }
  switch (state.kind) {
    case 'checking':
      return <Typography variant="body2">{t('update.checking')}</Typography>;
    case 'checkFailed':
      return (
        <Box sx={{ display: 'flex', alignItems: 'center', gap: 1 }}>
          <Typography variant="body2" color="error">
            {t('update.checkFailed')}
          </Typography>
          <Button size="small" onClick={() => checkNow(true)}>
            {t('update.retry')}
          </Button>
        </Box>
      );
    case 'upToDate':
      return (
        <Typography variant="body2">
          {t('update.latest')}
          {lastCheck && (
            <Typography component="span" variant="body2" color="text.secondary" sx={{ ml: 1 }}>
              {t('update.lastChecked', { time: formatCheckedAt(lastCheck, i18n.language) })}
            </Typography>
          )}
        </Typography>
      );
    default:
      return null;
  }
}

interface Project {
  id: string;
  name: string;
  tagline?: Record<string, string>;
  links?: { repo?: string };
  screenshots?: Array<{ src?: Record<string, string> }>;
}

/** The author's other projects, from apex-code's project list; left out when it cannot be read. */
function OtherProjects() {
  const { t, i18n } = useTranslation();
  const [projects, setProjects] = useState<Project[] | null>(null);
  const lang = i18n.language.startsWith('zh') ? 'zh-TW' : 'en';
  const pick = (value?: Record<string, string>) => value?.[lang] ?? value?.en ?? '';

  useEffect(() => {
    let live = true;
    fetch(PROJECTS_URL)
      .then((res) => (res.ok ? res.json() : Promise.reject(new Error(`HTTP ${res.status}`))))
      .then((data: { projects?: Project[] }) => {
        // Fields and values it does not know are left alone (apex-code docs/API.md).
        const others = (data.projects ?? []).filter((p) => p.id !== 'zoo-downloader' && p.links?.repo);
        if (live) setProjects(others);
      })
      .catch((e) => console.warn('[about] project list:', e instanceof Error ? e.message : e));
    return () => {
      live = false;
    };
  }, []);

  if (!projects?.length) return null;
  return (
    <Box>
      <Typography variant="subtitle2" color="text.secondary" sx={{ mb: 1 }}>
        {t('about.otherProjects')}
      </Typography>
      <Box sx={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(220px, 1fr))', gap: 1.5 }}>
        {projects.map((p) => {
          const cover = pick(p.screenshots?.[0]?.src);
          return (
            <Card key={p.id} variant="outlined">
              <CardActionArea href={p.links!.repo!} target="_blank" rel="noreferrer" sx={{ height: '100%' }}>
                {cover ? (
                  <CardMedia component="img" image={cover} alt="" loading="lazy" sx={{ aspectRatio: '16 / 10', objectFit: 'cover' }} />
                ) : (
                  <Box sx={{ aspectRatio: '16 / 10', bgcolor: 'action.hover' }} />
                )}
                <CardContent sx={{ p: 1.5, '&:last-child': { pb: 1.5 } }}>
                  <Typography variant="body2" sx={{ fontWeight: 600 }}>
                    {p.name}
                  </Typography>
                  <Typography variant="caption" color="text.secondary">
                    {pick(p.tagline)}
                  </Typography>
                </CardContent>
              </CardActionArea>
            </Card>
          );
        })}
      </Box>
    </Box>
  );
}
