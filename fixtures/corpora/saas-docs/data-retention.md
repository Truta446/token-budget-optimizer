# Data retention and deletion

- Application logs are retained for 14 days on Starter, 30 days on Team and
  up to 1 year on Enterprise (configurable).
- Audit logs (Enterprise only) are retained for 400 days.
- Backups are encrypted with AES-256 and kept for 35 days.

## Deleting a workspace
Workspace owners can delete a workspace from *Settings → Danger zone*. Data is
soft-deleted immediately and permanently erased after 30 days, including
backups after their 35-day rotation. Deletion cannot be undone after 30 days.

## Data residency
Enterprise workspaces can choose the EU (Frankfurt) or US (Virginia) region at
creation time. The region cannot be changed later.
