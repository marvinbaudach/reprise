import { permalink, treelink } from './measurements';

export interface ArchitectureSurface {
  readonly id: 'gnome' | 'android' | 'cli' | 'mcp';
  readonly name: string;
  readonly stack: string;
  readonly role: string;
  readonly adapter: string;
  readonly href: string;
}

export const ARCHITECTURE_SURFACES: readonly ArchitectureSurface[] = [
  {
    id: 'gnome',
    name: 'GNOME',
    stack: 'GTK4 · libadwaita',
    role: 'native desktop',
    adapter: 'reprise-platform-linux',
    href: treelink('crates/reprise-gnome'),
  },
  {
    id: 'android',
    name: 'Android',
    stack: 'Kotlin · Compose',
    role: 'native mobile',
    adapter: 'reprise-android-ffi',
    href: treelink('android'),
  },
  {
    id: 'cli',
    name: 'CLI',
    stack: 'Rust · terminal',
    role: 'headless surface',
    adapter: 'core facade',
    href: treelink('crates/reprise-cli'),
  },
  {
    id: 'mcp',
    name: 'MCP',
    stack: 'Rust · JSON-RPC',
    role: 'agent surface',
    adapter: 'capability gate',
    href: treelink('crates/reprise-mcp'),
  },
] as const;

export const ARCHITECTURE_LINKS = {
  view: treelink('crates/reprise-view'),
  core: permalink('crates/reprise-core/Cargo.toml'),
  gate: permalink('scripts/check-architecture.sh'),
} as const;
