import { describe, expect, test } from 'vitest';
import { DAY_MS, checkDueIn, failureReason, formatCheckedAt, majorMinor, newerBy, offerOnMainScreen, parseVersion } from '../utils/appUpdate';

describe('versions', () => {
  test('only major.minor.patch counts', () => {
    expect(parseVersion('0.7.0')).toEqual([0, 7, 0]);
    expect(parseVersion('v1.2.3')).toEqual([1, 2, 3]);
    expect(parseVersion('0.7.0-rc.1')).toBeNull();
    expect(parseVersion('0.7')).toBeNull();
    expect(parseVersion(null)).toBeNull();
  });

  test('how much newer', () => {
    expect(newerBy('0.6.4', '0.7.0')).toBe('minor');
    expect(newerBy('0.6.4', '1.0.0')).toBe('minor');
    expect(newerBy('0.6.4', '0.6.5')).toBe('patch');
    expect(newerBy('0.6.4', '0.6.4')).toBe('none');
    expect(newerBy('0.7.0', '0.6.9')).toBe('none');
    expect(newerBy('1.0.0', '0.9.9')).toBe('none');
    expect(newerBy('0.6.4', '0.8.0-rc.1')).toBe('none');
    expect(newerBy('0.6.4', 'garbage')).toBe('none');
  });
});

describe('main screen', () => {
  test('offers a new major or minor only', () => {
    expect(offerOnMainScreen('0.6.4', '0.7.0', null)).toBe(true);
    expect(offerOnMainScreen('0.6.4', '0.6.5', null)).toBe(false);
    expect(offerOnMainScreen('0.6.4', null, null)).toBe(false);
  });

  test('skipping 0.7 skips every 0.7.x, not 0.8', () => {
    expect(majorMinor('0.7.0')).toBe('0.7');
    expect(offerOnMainScreen('0.6.4', '0.7.0', '0.7')).toBe(false);
    expect(offerOnMainScreen('0.6.4', '0.7.3', '0.7')).toBe(false);
    expect(offerOnMainScreen('0.6.4', '0.8.0', '0.7')).toBe(true);
  });
});

describe('when to check', () => {
  const now = Date.parse('2026-11-28T12:00:00Z');
  test('a check from the last day is reused until it is a day old', () => {
    expect(checkDueIn('2026-11-28T10:00:00Z', now)).toBe(DAY_MS - 2 * 60 * 60 * 1000);
    expect(checkDueIn('2026-11-27T11:00:00Z', now)).toBe(0);
  });
  test('never checked, unreadable, or in the future: due now', () => {
    expect(checkDueIn(null, now)).toBe(0);
    expect(checkDueIn('yesterday', now)).toBe(0);
    expect(checkDueIn('2026-11-29T12:00:00Z', now)).toBe(0);
  });
});

describe('failures', () => {
  test('signature and network failures are named', () => {
    expect(failureReason('signature verification failed')).toBe('signature');
    expect(failureReason('error sending request for url (https://github.com/...)')).toBe('network');
    expect(failureReason('something else')).toBeNull();
  });
});

describe('last checked', () => {
  const now = new Date(2026, 10, 28, 15, 0);
  test('today and yesterday by name, 24-hour clock', () => {
    expect(formatCheckedAt(new Date(2026, 10, 28, 14, 31).toISOString(), 'en', now)).toBe('today 14:31');
    expect(formatCheckedAt(new Date(2026, 10, 27, 9, 5).toISOString(), 'en', now)).toBe('yesterday 09:05');
    expect(formatCheckedAt(new Date(2026, 10, 28, 14, 31).toISOString(), 'zh-TW', now)).toBe('今天 14:31');
  });
  test('older: the date', () => {
    expect(formatCheckedAt(new Date(2026, 10, 20, 8, 0).toISOString(), 'en', now)).toContain('08:00');
  });
});
