// The settings page's building blocks: sections picked on the left, groups of rows in outlined
// boxes, one setting per row (what it is on the left, its control on the right).
import React, { useState } from 'react';
import { Box, Divider, List, ListItemButton, ListItemText, Paper, Stack, Typography } from '@mui/material';

export interface Section {
  id: string;
  label: string;
  content: React.ReactNode;
}

/** The section each panel showed last, so coming back to a tab keeps it. */
const lastSection = new Map<string, string>();

/** Sections of a settings tab, picked from a list on the left. */
export function SectionedPanel({ id, sections }: { id: string; sections: Section[] }) {
  const [current, setCurrent] = useState(() => lastSection.get(id) ?? sections[0].id);
  const shown = sections.find((s) => s.id === current) ?? sections[0];
  const pick = (next: string) => {
    lastSection.set(id, next);
    setCurrent(next);
  };

  return (
    <Box sx={{ display: 'flex', gap: 3, alignItems: 'flex-start' }}>
      <List dense disablePadding sx={{ width: 168, flexShrink: 0, position: 'sticky', top: 0 }}>
        {sections.map((s) => (
          <ListItemButton key={s.id} selected={s.id === shown.id} onClick={() => pick(s.id)} sx={{ borderRadius: 1, mb: 0.25 }}>
            <ListItemText primary={s.label} slotProps={{ primary: { noWrap: true } }} />
          </ListItemButton>
        ))}
      </List>
      <Box sx={{ flex: 1, minWidth: 0 }}>{shown.content}</Box>
    </Box>
  );
}

/** Rows that belong together, in one outlined box under an optional title. */
export function SettingsGroup({ title, children }: { title?: string; children: React.ReactNode }) {
  return (
    <Box sx={{ mb: 3 }}>
      {title && (
        <Typography variant="subtitle2" color="text.secondary" sx={{ mb: 1 }}>
          {title}
        </Typography>
      )}
      <Paper variant="outlined">
        <Stack divider={<Divider />}>{React.Children.toArray(children).filter(Boolean)}</Stack>
      </Paper>
    </Box>
  );
}

/** One setting: its name (and a line about it) on the left, its control on the right. */
export function SettingRow({
  label,
  description,
  control,
  disabled,
  children,
}: {
  label: React.ReactNode;
  description?: React.ReactNode;
  control?: React.ReactNode;
  disabled?: boolean;
  /** Under the row, the full width: a field too wide for the right side. */
  children?: React.ReactNode;
}) {
  return (
    <Box sx={{ px: 2, py: 1.5 }}>
      <Box sx={{ display: 'flex', alignItems: 'center', gap: 2, minHeight: 40 }}>
        <Box sx={{ flex: 1, minWidth: 0 }}>
          <Typography variant="body2" component="div" color={disabled ? 'text.disabled' : 'text.primary'}>
            {label}
          </Typography>
          {description && (
            <Typography variant="caption" color={disabled ? 'text.disabled' : 'text.secondary'} component="div">
              {description}
            </Typography>
          )}
        </Box>
        {control && <Box sx={{ flexShrink: 0, display: 'flex', alignItems: 'center', gap: 1 }}>{control}</Box>}
      </Box>
      {children && <Box sx={{ mt: 1 }}>{children}</Box>}
    </Box>
  );
}
