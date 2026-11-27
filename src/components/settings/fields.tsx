// Typed settings save when the field is left (or Enter), not on every key.
import { useEffect, useState } from 'react';
import { InputAdornment, MenuItem, Select, TextField } from '@mui/material';

/** A whole number; empty is `null` when `allowEmpty`. Out of range or not a number: put back. */
export function NumberField({
  value,
  onCommit,
  min,
  max,
  unit,
  allowEmpty,
  placeholder,
  disabled,
  width = 140,
}: {
  value: number | null;
  onCommit: (value: number | null) => void;
  min: number;
  max?: number;
  unit?: string;
  allowEmpty?: boolean;
  placeholder?: string;
  disabled?: boolean;
  width?: number;
}) {
  const [text, setText] = useState(value?.toString() ?? '');
  useEffect(() => setText(value?.toString() ?? ''), [value]);

  const commit = () => {
    if (text.trim() === '' && allowEmpty) {
      if (value !== null) onCommit(null);
      return;
    }
    const n = Number(text);
    if (Number.isInteger(n) && n >= min && (max === undefined || n <= max)) {
      if (n !== value) onCommit(n);
    } else {
      setText(value?.toString() ?? '');
    }
  };

  return (
    <TextField
      size="small"
      type="number"
      value={text}
      disabled={disabled}
      placeholder={placeholder}
      onChange={(e) => setText(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => e.key === 'Enter' && (e.target as HTMLInputElement).blur()}
      sx={{ width }}
      slotProps={{
        htmlInput: { min, max },
        input: unit ? { endAdornment: <InputAdornment position="end">{unit}</InputAdornment> } : undefined,
      }}
    />
  );
}

/** A choice among a few: the settings page's dropdown. */
export function ChoiceField<T extends string>({
  value,
  options,
  onChange,
  disabled,
  minWidth = 200,
}: {
  value: T;
  options: Array<{ value: T; label: string; disabled?: boolean }>;
  onChange: (value: T) => void;
  disabled?: boolean;
  minWidth?: number;
}) {
  return (
    <Select size="small" value={value} disabled={disabled} onChange={(e) => onChange(e.target.value as T)} sx={{ minWidth }}>
      {options.map((o) => (
        <MenuItem key={o.value} value={o.value} disabled={o.disabled}>
          {o.label}
        </MenuItem>
      ))}
    </Select>
  );
}
