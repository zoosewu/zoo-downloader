// What a failed task's error means, in the user's language.
import type { TFunction } from 'i18next';
import type { CoreError } from '../backend';
import { formatBytes } from './downloadCalculations';

/** A sentence for `error`; the backend's own detail is appended where it helps. */
export function errorText(error: CoreError | null, t: TFunction): string {
  if (!error) return '';
  switch (error.kind) {
    case 'resolve':
    case 'siteLimit':
      return error.detail.hint ? `${error.detail.message} (${error.detail.hint})` : error.detail.message;
    case 'http':
      return t('errors.core.http', { status: error.detail });
    case 'insufficientSpace':
      return t('errors.core.insufficientSpace', {
        needed: formatBytes(error.detail.needed),
        available: formatBytes(error.detail.available),
      });
    case 'cancelled':
    case 'notFound':
      return t(`errors.core.${error.kind}`);
    default:
      return `${t(`errors.core.${error.kind}`)}: ${error.detail}`;
  }
}
