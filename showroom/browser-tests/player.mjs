import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { homedir } from 'node:os';
import { join } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

// Browser verification is opt-in: serve the showroom first, then run this file.
// No dependency or personal browser profile is used.
const output = join(homedir(), '.cache/reprise-scratch/showcase-calm-player');
await mkdir(output, { recursive: true });
const profile = await mkdtemp(join(output, 'browser-'));
const browser = spawn(
  process.env.CHROMIUM ?? '/usr/bin/chromium',
  [
    '--headless=new',
    '--disable-gpu',
    '--no-first-run',
    '--disable-extensions',
    '--remote-debugging-port=0',
    `--user-data-dir=${profile}`,
    'about:blank',
  ],
  { stdio: 'ignore' },
);
let socket;
let serial = 0;
const pending = new Map();
const errors = [];
async function until(read, accept, label) {
  for (let attempt = 0; attempt < 120; attempt += 1) {
    const value = await read();
    if (accept(value)) return value;
    await delay(100);
  }
  throw new Error(`Timed out: ${label}`);
}
function call(method, params = {}) {
  const id = ++serial;
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      pending.delete(id);
      reject(new Error(method));
    }, 15000);
    pending.set(id, { resolve, reject, timer });
    socket.send(JSON.stringify({ id, method, params }));
  });
}
async function evaluate(expression) {
  const reply = await call('Runtime.evaluate', {
    expression,
    returnByValue: true,
    awaitPromise: true,
    userGesture: true,
  });
  if (reply.exceptionDetails) throw new Error(JSON.stringify(reply.exceptionDetails));
  return reply.result.value;
}
async function screenshot(name) {
  const { data } = await call('Page.captureScreenshot', { format: 'png' });
  await writeFile(join(output, name), Buffer.from(data, 'base64'));
}
async function phoneFrame() {
  return evaluate('document.querySelector(".hero-product__phone canvas")?.toDataURL() ?? null');
}
async function assertPhoneAnimates(label) {
  await until(() => phoneFrame(), Boolean, `${label}: phone canvas exists`);
  const first = await phoneFrame();
  await until(
    () => phoneFrame(),
    (frame) => frame !== first,
    `${label}: phone visualization moves`,
  );
  const running = await phoneFrame();
  await until(
    () => phoneFrame(),
    (frame) => frame !== running,
    `${label}: phone visualization keeps moving after its first frame`,
  );
  assert.ok(
    await evaluate(`(() => {
      const phone = document.querySelector('.hero-product__phone').getBoundingClientRect();
      const canvas = document.querySelector('.hero-product__phone canvas').getBoundingClientRect();
      return canvas.width > 0 && canvas.height > 0 && canvas.left >= phone.left
        && canvas.right <= phone.right && canvas.top >= phone.top && canvas.bottom <= phone.bottom;
    })()`),
    `${label}: visualization stays inside the phone`,
  );
}
try {
  const port = await until(
    async () => {
      try {
        return (await readFile(join(profile, 'DevToolsActivePort'), 'utf8')).split('\n')[0];
      } catch {
        return null;
      }
    },
    Boolean,
    'Chromium endpoint',
  );
  const targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
  socket = new WebSocket(targets.find((target) => target.type === 'page').webSocketDebuggerUrl);
  await new Promise((resolve) => socket.addEventListener('open', resolve, { once: true }));
  socket.addEventListener('message', ({ data }) => {
    const message = JSON.parse(data);
    if (message.method === 'Runtime.exceptionThrown') errors.push(message.params);
    const request = pending.get(message.id);
    if (!request) return;
    pending.delete(message.id);
    clearTimeout(request.timer);
    if (message.error) request.reject(new Error(JSON.stringify(message.error)));
    else request.resolve(message.result);
  });
  await call('Runtime.enable');
  await call('Emulation.setDeviceMetricsOverride', {
    width: 1440,
    height: 1000,
    deviceScaleFactor: 1,
    mobile: false,
  });
  await call('Page.navigate', {
    url: process.env.SHOWROOM_URL ?? 'http://127.0.0.1:4175/reprise/',
  });
  await until(
    () => evaluate('document.readyState'),
    (state) => state === 'complete',
    'page loaded',
  );
  await delay(500);
  assert.equal(
    await evaluate('!!document.querySelector(\'button[aria-label="Play film"]\')'),
    true,
    'a central play button must invite playback',
  );
  assert.equal(
    await evaluate('!!document.querySelector(\'input[aria-label="Seek film"]\')'),
    true,
    'the film must have a seek control',
  );
  assert.equal(await evaluate('document.querySelector("video").paused'), true);
  assert.equal(
    await evaluate('document.querySelector("video").readyState'),
    0,
    'media stays unfetched before play',
  );
  assert.equal(
    await evaluate('document.querySelector(".film__error")?.textContent ?? null'),
    null,
    'no false error before playback',
  );
  assert.ok(
    await evaluate(
      'document.querySelector("#film").getBoundingClientRect().top < document.querySelector("#ch-01").getBoundingClientRect().top',
    ),
  );
  await assertPhoneAnimates('desktop');
  await screenshot('desktop.png');
  console.log('PASS: player is discoverable and waits for an explicit start');
  await evaluate('document.querySelector("#film").scrollIntoView({behavior:"instant"})');
  await screenshot('player-poster.png');
  const offscreenPhone = await phoneFrame();
  await delay(350);
  assert.equal(await phoneFrame(), offscreenPhone, 'offscreen phone stops drawing');
  await evaluate('document.querySelector(\'button[aria-label="Play film"]\').click()');
  await until(
    () => evaluate('document.querySelector("video").currentTime'),
    (value) => value > 0.2,
    'film playing',
  );
  await evaluate('document.querySelector(\'button[aria-label="Pause"]\').click()');
  assert.equal(await evaluate('document.querySelector("video").paused'), true);
  await evaluate(`(() => {
    const slider = document.querySelector('input[aria-label="Seek film"]');
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(slider, '20');
    slider.dispatchEvent(new Event('input', {bubbles:true}));
    slider.dispatchEvent(new Event('change', {bubbles:true}));
  })()`);
  await until(
    () => evaluate('document.querySelector("video").currentTime'),
    (value) => Math.abs(value - 20) < 0.2,
    'seek while paused',
  );
  assert.equal(await evaluate('document.querySelector("video").paused'), true);
  await evaluate('document.querySelector(\'button[aria-label="Mute"]\').click()');
  assert.equal(await evaluate('document.querySelector("video").muted'), true);
  assert.equal(
    await evaluate('document.querySelector("video").paused'),
    true,
    'mute must not start a paused film',
  );
  await evaluate('document.querySelector(\'button[aria-label="Full screen"]\').click()');
  await until(
    () => evaluate('!!document.querySelector(\'button[aria-label="Exit full screen"]\')'),
    Boolean,
    'fullscreen entered',
  );
  await evaluate('document.querySelector(\'button[aria-label="Exit full screen"]\').click()');
  await until(() => evaluate('!document.fullscreenElement'), Boolean, 'fullscreen exited');
  await evaluate('document.querySelector(".film__screen").focus()');
  await call('Input.dispatchKeyEvent', { type: 'keyDown', key: 'ArrowRight', code: 'ArrowRight' });
  await call('Input.dispatchKeyEvent', { type: 'keyUp', key: 'ArrowRight', code: 'ArrowRight' });
  await until(
    () => evaluate('document.querySelector("video").currentTime'),
    (value) => Math.abs(value - 25) < 0.2,
    'keyboard seek',
  );
  await screenshot('player-paused.png');
  await evaluate(`(() => {
    const volume = document.querySelector('input[aria-label="Volume"]');
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(volume, '.5');
    volume.dispatchEvent(new Event('input', {bubbles:true}));
    volume.dispatchEvent(new Event('change', {bubbles:true}));
  })()`);
  assert.equal(await evaluate('document.querySelector("video").volume'), 0.5);
  assert.equal(await evaluate('document.querySelector("video").muted'), false);
  await evaluate('document.querySelector(\'button[aria-label="Play"]\').click()');
  await evaluate('document.activeElement.blur()');
  await until(
    () => evaluate('getComputedStyle(document.querySelector(".film__controls")).opacity'),
    (value) => value === '0',
    'playing controls settle out of view',
  );
  await evaluate('document.querySelector(\'input[aria-label="Seek film"]\').focus()');
  await call('Input.dispatchKeyEvent', { type: 'keyDown', key: 'ArrowRight', code: 'ArrowRight' });
  await call('Input.dispatchKeyEvent', { type: 'keyUp', key: 'ArrowRight', code: 'ArrowRight' });
  await until(
    () => evaluate('getComputedStyle(document.querySelector(".film__controls")).opacity'),
    (value) => value === '1',
    'keyboard focus reveals controls',
  );
  await evaluate('document.querySelector("video").pause()');
  await until(
    () => evaluate('!!document.querySelector(\'button[aria-label="Play"]\')'),
    Boolean,
    'paused toolbar',
  );
  await evaluate('document.querySelector(\'button[aria-label="Play"]\').click()');
  await evaluate(
    'document.querySelector("video").currentTime = document.querySelector("video").duration - .2',
  );
  await until(
    () => evaluate('!!document.querySelector(\'button[aria-label="Watch again"]\')'),
    Boolean,
    'replay offered',
  );
  assert.equal(await evaluate('document.querySelector("video").paused'), true);
  assert.ok(
    await evaluate(
      'document.querySelector("video").currentTime < document.querySelector("video").duration - .5',
    ),
    'film rests before the black fade',
  );
  assert.equal(
    await evaluate('document.querySelector(".film__time").textContent.trim()'),
    '0:58 / 0:58',
    'finished playback reports the full duration while holding the end card',
  );
  await evaluate('document.querySelector(\'button[aria-label="Watch again"]\').click()');
  await until(
    () => evaluate('document.querySelector("video").currentTime'),
    (value) => value > 0 && value < 2,
    'replay from beginning',
  );
  await evaluate('document.querySelector("video").pause()');
  assert.deepEqual(errors, [], 'no uncaught browser exceptions');
  console.log(
    'PASS: play, pause, seek, volume, keyboard, fullscreen, control visibility, end card and replay',
  );
  for (const width of [390, 320, 768, 1440]) {
    await call('Emulation.setDeviceMetricsOverride', {
      width,
      height: 844,
      deviceScaleFactor: 1,
      mobile: width < 500,
    });
    await call('Emulation.setTouchEmulationEnabled', { enabled: width < 500 });
    if (width === 390) {
      await call('Page.navigate', {
        url: process.env.SHOWROOM_URL ?? 'http://127.0.0.1:4175/reprise/',
      });
      await until(
        () => evaluate('!!document.querySelector(\'button[aria-label="Play film"]\')'),
        Boolean,
        'fresh mobile player',
      );
      await delay(300);
    }
    await evaluate('window.scrollTo({top:0,behavior:"instant"})');
    await delay(250);
    assert.ok(
      await evaluate('document.documentElement.scrollWidth <= innerWidth'),
      `no horizontal overflow at ${width}`,
    );
    assert.ok(
      await evaluate(`(() => {
        const table = document.querySelector('.ledger').getBoundingClientRect();
        const caption = document.querySelector('.ledger caption').getBoundingClientRect();
        return caption.width >= table.width * 0.95;
      })()`),
      `measurement explanation uses the reading width at ${width}`,
    );
    if (width === 390) {
      assert.ok(
        await evaluate(
          'document.querySelector(".hero-product").getBoundingClientRect().top < innerHeight',
        ),
        'product appears on the first mobile screen',
      );
      await assertPhoneAnimates('mobile');
      await screenshot('mobile.png');
    }
    await evaluate('document.querySelector("#film").scrollIntoView({behavior:"instant"})');
    await delay(250);
    if (width === 390) {
      await screenshot('mobile-player.png');
      const point = await evaluate(
        `(() => { const r=document.querySelector('.film__big-play').getBoundingClientRect(); return {x:r.x+r.width/2,y:r.y+r.height/2}; })()`,
      );
      await call('Input.dispatchTouchEvent', {
        type: 'touchStart',
        touchPoints: [{ ...point, radiusX: 1, radiusY: 1 }],
      });
      await call('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
      await until(
        () => evaluate('document.querySelector("video").currentTime'),
        (value) => value > 0.2,
        'touch starts playback',
      );
      assert.ok(
        await evaluate('document.querySelector("video").currentSrc.includes("720")'),
        'mobile selects the smaller encode',
      );
      const pausePoint = await evaluate(`(() => {
        const r = document.querySelector('.film__toolbar button[aria-label="Pause"]').getBoundingClientRect();
        return { x: r.x + r.width / 2, y: r.y + r.height / 2 };
      })()`);
      await call('Input.dispatchTouchEvent', {
        type: 'touchStart',
        touchPoints: [pausePoint],
      });
      await call('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
      await until(
        () => evaluate('document.querySelector("video").paused'),
        Boolean,
        'touch pauses',
      );
      const track = await evaluate(`(() => {
        const v = document.querySelector('video');
        const r = document.querySelector('.film__seek').getBoundingClientRect();
        return { x: r.x + 6 + (r.width - 12) * v.currentTime / v.duration,
          end: r.x + r.width * 0.65, y: r.y + r.height / 2 };
      })()`);
      await call('Input.dispatchTouchEvent', {
        type: 'touchStart',
        touchPoints: [{ x: track.x, y: track.y }],
      });
      for (let step = 1; step <= 8; step += 1) {
        await call('Input.dispatchTouchEvent', {
          type: 'touchMove',
          touchPoints: [
            {
              x: track.x + ((track.end - track.x) * step) / 8,
              y: track.y,
            },
          ],
        });
        await delay(25);
      }
      await call('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
      await until(
        () => evaluate('document.querySelector("video").currentTime'),
        (value) => value > 30 && value < 45,
        'finger drag seeks',
      );
      assert.ok(
        await evaluate('document.querySelector("video").paused'),
        'touch seeking preserves pause',
      );
      await screenshot('mobile-player-seek.png');
    }
    const controls = await evaluate(`(() => {
      const frame = document.querySelector('.film__screen').getBoundingClientRect();
      return [...document.querySelectorAll('.film__toolbar button, .film__toolbar input')].every((item) => {
        const rect = item.getBoundingClientRect();
        return rect.left >= frame.left && rect.right <= frame.right + 1;
      });
    })()`);
    assert.ok(controls, `controls fit at ${width}`);
  }
  await call('Emulation.setDeviceMetricsOverride', {
    width: 844,
    height: 390,
    deviceScaleFactor: 1,
    mobile: true,
  });
  await evaluate(
    'document.querySelector(".film__screen").scrollIntoView({block:"end",behavior:"instant"})',
  );
  await delay(250);
  assert.ok(
    await evaluate(`(() => {
      const frame = document.querySelector('.film__screen').getBoundingClientRect();
      const header = document.querySelector('.site-header').getBoundingClientRect();
      return frame.top >= header.bottom && frame.bottom <= innerHeight + 1;
    })()`),
    'landscape keeps the complete player below the fixed header',
  );
  await screenshot('mobile-landscape.png');
  await evaluate('document.querySelector(".film__fullscreen").click()');
  await until(() => evaluate('!!document.fullscreenElement'), Boolean, 'landscape fullscreen');
  assert.ok(
    await evaluate(
      'document.querySelector("video").getBoundingClientRect().height >= innerHeight - 1',
    ),
    'full screen uses the complete landscape height',
  );
  await evaluate('document.exitFullscreen()');
  await call('Emulation.setDeviceMetricsOverride', {
    width: 1440,
    height: 844,
    deviceScaleFactor: 1,
    mobile: false,
  });
  await evaluate('document.querySelector("#ch-01 details").open=true');
  await evaluate('document.querySelector("#ch-01").scrollIntoView({behavior:"instant"})');
  await delay(250);
  assert.ok(
    await evaluate(
      '[...document.querySelectorAll("[data-ratio] > span")].every(x => x.getBoundingClientRect().width > 0)',
    ),
    'source-derived ratio is drawn when opened',
  );
  await screenshot('architecture.png');
  await evaluate('document.querySelector("#ch-05").scrollIntoView({behavior:"instant"})');
  await until(
    () => evaluate('document.querySelector("[data-navlink][aria-current]")?.hash'),
    (hash) => hash === '#ch-05',
    'navigation follows the reordered performance chapter',
  );
  await screenshot('performance.png');
  await call('Emulation.setEmulatedMedia', {
    features: [{ name: 'prefers-reduced-motion', value: 'reduce' }],
  });
  await evaluate('window.scrollTo({top:0,behavior:"instant"})');
  await delay(250);
  const reducedPhone = await phoneFrame();
  await delay(350);
  assert.equal(await phoneFrame(), reducedPhone, 'reduced motion keeps a still phone frame');
  assert.ok(
    await evaluate(
      '[...document.querySelectorAll("h1, h2, [data-counter]")].filter(x => x.getClientRects().length).every(x => getComputedStyle(x).opacity === "1")',
    ),
    'content stays readable under reduced motion',
  );
  await call('Network.enable');
  await call('Network.setBlockedURLs', { urls: ['*.webm', '*.mp4'] });
  await call('Page.navigate', {
    url: process.env.SHOWROOM_URL ?? 'http://127.0.0.1:4175/reprise/',
  });
  await until(
    () => evaluate('!!document.querySelector(\'button[aria-label="Play film"]\')'),
    Boolean,
    'player for unavailable media',
  );
  await evaluate('document.querySelector(\'button[aria-label="Play film"]\').click()');
  await until(
    () => evaluate('document.querySelector(".film__error")?.textContent ?? ""'),
    (value) => value.includes('Open video'),
    'unavailable media offers recovery',
  );
  assert.equal(
    await evaluate('!!document.querySelector(".film__loading")'),
    false,
    'unavailable media does not load forever',
  );
  await call('Network.setBlockedURLs', { urls: [] });
  await evaluate('document.querySelector(\'button[aria-label="Play film"]\').click()');
  await until(
    () => evaluate('document.querySelector("video").currentTime'),
    (value) => value > 0.2,
    'retry recovers without a reload',
  );
  await evaluate('document.querySelector("video").pause()');
  assert.deepEqual(errors, []);
  console.log(
    'PASS: responsive layout at 320, 390, 768 and 1440 px, static figures and reduced motion',
  );
} finally {
  socket?.close();
  browser.kill();
  await new Promise((resolve) => browser.once('exit', resolve));
  await rm(profile, { recursive: true, force: true, maxRetries: 5, retryDelay: 200 });
}
