#!/usr/bin/env python3
"""Build a playlist through Reprise's own MCP server, the way an agent would.

This is the working half of the MCP shot. The film shows the request and then
the result; this is what happens in between, and it happens over the real
stdio protocol against the real binary — nothing here writes to the database
directly, because a shot of a faked result would be a lie about the feature.

The server has no "similar artists" tool, and that is the honest shape of the
task: the model knows who sounds like Lorna Shore, the library knows which of
them it holds, and the server does the writing. Each side does what only it can.
"""
import json
import os
import subprocess
import sys

# What an agent brings to the request: nobody stored this, it is knowledge about
# the genre. The library then decides which of these it actually has — every
# name below was checked against it, and the ones it does not hold drop out on
# their own rather than being pruned here.
SEED = 'Lorna Shore'
NEIGHBOURS = {
    'Lorna Shore': [
        'Immortal Disfigurement', 'Shadow of Intent', 'Chelsea Grin',
        'Carnifex', 'Emmure', 'Distant',
        'Signs of the Swarm', 'Brand of Sacrifice', "Humanity's Last Breath",
        'Make Them Suffer', 'Whitechapel', 'Thy Art Is Murder', 'Ingested',
        'Kublai Khan', 'Bodysnatcher', 'Slaughter To Prevail',
    ],
    # A second seed, because the first one's playlist is already in the library
    # and on the phone: a shot of a row appearing must not appear beside a row
    # that reads almost the same. This one is melodic where the other is
    # deathcore, so the two are also audibly different answers.
    'Bring Me The Horizon': [
        'Asking Alexandria', 'A Day to Remember', 'Bury Tomorrow',
        'Blessthefall', 'Motionless In White', 'Falling in Reverse',
        'We Came As Romans', 'Miss May I', 'The Devil Wears Prada',
        'Wage War', 'Currents', 'Annisokay', 'Killswitch Engage',
        'Caliban', 'Our Last Night', 'The Word Alive', 'Electric Callboy',
    ],
}
# How many tracks the answer holds. Small is a virtue in the film: the shot is
# the row arriving, not the length of the list, and a short playlist is written
# and read back in a moment instead of holding the take open.
TARGET = int(os.environ.get('SHOWREEL_MCP_TARGET', '100'))


class Server:
    def __init__(self, binary, db):
        self.proc = subprocess.Popen(
            [binary, '--db', db],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
            text=True, bufsize=1)
        self.next_id = 0

    def call(self, method, params=None):
        self.next_id += 1
        request = {'jsonrpc': '2.0', 'id': self.next_id, 'method': method}
        if params is not None:
            request['params'] = params
        self.proc.stdin.write(json.dumps(request) + '\n')
        self.proc.stdin.flush()
        while True:
            line = self.proc.stdout.readline()
            if not line:
                raise SystemExit(f'{method}: the server closed the pipe')
            message = json.loads(line)
            if message.get('id') == self.next_id:
                if 'error' in message:
                    raise SystemExit(f'{method}: {message["error"]}')
                return message.get('result', {})

    def notify(self, method, params=None):
        """A notification carries no id and gets no answer — sending one with an
        id makes the server reject `notifications/initialized` as an unknown
        method, which looks like a protocol mismatch and is not one."""
        message = {'jsonrpc': '2.0', 'method': method}
        if params is not None:
            message['params'] = params
        self.proc.stdin.write(json.dumps(message) + '\n')
        self.proc.stdin.flush()

    def tool(self, name, arguments):
        """The payload lives in `structuredContent`. The text block beside it is
        a human sentence ("3 of 17 matching track(s)"), not JSON — parsing that
        is what a first pass gets wrong."""
        result = self.call('tools/call', {'name': name, 'arguments': arguments})
        if result.get('isError'):
            raise SystemExit(f'{name}: {result.get("content")}')
        return result.get('structuredContent', result)

    def close(self):
        self.proc.stdin.close()
        self.proc.terminate()


def main():
    binary = sys.argv[1]
    db = sys.argv[2]
    seed = sys.argv[4] if len(sys.argv) > 4 else SEED
    if seed not in NEIGHBOURS:
        raise SystemExit(f'no neighbours known for {seed!r} — '
                         f'have {sorted(NEIGHBOURS)}')
    name = sys.argv[3] if len(sys.argv) > 3 else f'Like {seed}'

    server = Server(binary, db)
    server.call('initialize', {
        'protocolVersion': '2024-11-05',
        'capabilities': {},
        'clientInfo': {'name': 'reprise-showreel', 'version': '1'},
    })
    server.notify('notifications/initialized')

    pools = {}
    for artist in [seed] + NEIGHBOURS[seed]:
        found = server.tool('music_search_tracks', {'query': artist, 'limit': 120})
        tracks = found.get('tracks', found if isinstance(found, list) else [])
        picked = [t['id'] for t in tracks if str(t.get('artist', '')).lower() == artist.lower()]
        if picked:
            pools[artist] = picked
        print(f'{artist:26s} {len(picked):3d} in library', file=sys.stderr)

    if not pools:
        raise SystemExit('no tracks matched — nothing to build a playlist from')

    # Round robin, not artist by artist. Taken in order, one artist with sixty
    # tracks would be most of the playlist and the rest would be a tail; taking
    # one from each in turn keeps the mix even and keeps the seed at the front.
    track_ids, order = [], list(pools)
    while len(track_ids) < TARGET and any(pools[a] for a in order):
        for artist in order:
            if not pools[artist]:
                continue
            track_ids.append(pools[artist].pop(0))
            if len(track_ids) >= TARGET:
                break

    # The count in the name is the count that was found, never the count that
    # was asked for — the overlay in the film quotes it.
    name = f'{name} · {len(track_ids)}'
    print(f'{len(pools)} artists, {len(track_ids)} tracks -> {name}', file=sys.stderr)
    made = server.tool('music_create_playlist', {'name': name, 'track_ids': track_ids})
    print(json.dumps(made))
    server.close()


if __name__ == '__main__':
    main()
