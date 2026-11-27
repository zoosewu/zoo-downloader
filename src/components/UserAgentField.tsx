// A User-Agent: type one, or pick a common one.
import { Autocomplete, TextField } from '@mui/material';
import { useTranslation } from 'react-i18next';
import { USER_AGENT_PRESETS } from '../utils/userAgents';

interface UserAgentFieldProps {
  value: string;
  onChange: (value: string) => void;
  /** Shown when empty: what is sent instead. */
  placeholder?: string;
  helperText?: string;
  /** Inside a row that already names it: no label, no margin. */
  bare?: boolean;
}

export default function UserAgentField({ value, onChange, placeholder, helperText, bare }: UserAgentFieldProps) {
  const { t } = useTranslation();
  return (
    <Autocomplete
      freeSolo
      options={USER_AGENT_PRESETS}
      getOptionLabel={(option) => (typeof option === 'string' ? option : option.value)}
      renderOption={(props, option) => (
        <li {...props} key={option.label}>
          {option.label}
        </li>
      )}
      inputValue={value}
      onInputChange={(_, next) => onChange(next)}
      renderInput={(params) => (
        <TextField
          {...params}
          label={bare ? undefined : t('headers.userAgent')}
          size={bare ? 'small' : undefined}
          placeholder={placeholder}
          helperText={helperText}
          margin={bare ? 'none' : 'normal'}
          fullWidth
        />
      )}
    />
  );
}
