export const CLI_COMMANDS = [
  { command: 'library summary', nowrap: true },
  { command: 'search "portishead"', nowrap: true },
  { command: 'playlist create "Focus"', nowrap: false },
  { command: 'scan ~/Music', nowrap: true },
  { command: 'instrumental create 481', nowrap: true },
  { command: 'jobs status --batch b-2f9c', nowrap: true },
  { command: 'events tail --since 0', nowrap: true },
  { command: 'concerts list --all --json', nowrap: true },
] as const;

export interface McpCapability {
  readonly id: string;
  readonly description: string;
  readonly enabled: boolean;
}

export const MCP_CAPABILITIES: readonly McpCapability[] = [
  {
    id: 'library:read',
    description: 'search tracks, artists, albums, playlists',
    enabled: true,
  },
  {
    id: 'playback:control',
    description: 'transport, volume, seek, queue',
    enabled: true,
  },
  { id: 'playlist:create', description: 'create a manual playlist', enabled: false },
  { id: 'playlist:manage', description: 'rename, append tracks', enabled: false },
  { id: 'sources:manage', description: 'podcasts, YouTube, radio', enabled: false },
  { id: 'device:sync', description: 'configure and run phone sync', enabled: false },
];
