/** Request limits for the public demo. Generous enough for real files, small
 * enough that one request cannot monopolise the process. */
export const LIMITS = {
  bodyBytes: 3 * 1024 * 1024,
  documents: 50,
  idChars: 200,
  documentChars: 400_000,
  totalChars: 1_500_000,
  queryChars: 2_000,
  maxTokens: 200_000,
  targetTokens: { min: 16, max: 4_000 },
  neighborHops: 4,
  requestsPerMinute: 60,
  previewChars: 200,
} as const;
