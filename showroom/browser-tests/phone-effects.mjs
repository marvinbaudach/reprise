import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { setTimeout as delay } from 'node:timers/promises';

const nativeScene = await readFile(
  new URL(
    '../../android/app/src/main/java/io/github/marvinbaudach/reprise/NowPlayingScene.kt',
    import.meta.url,
  ),
  'utf8',
);
const nativeSide = Number(nativeScene.match(/const val COVER_SIZE_DP = ([\d.]+)/)?.[1]);
const nativeRadius = Number(nativeScene.match(/const val COVER_RADIUS_DP = ([\d.]+)/)?.[1]);

async function assertNativePhoneCard(evaluate, scope) {
  const card = await evaluate(`(() => {
    const canvas = document.querySelector('${scope} canvas');
    const rect = canvas.getBoundingClientRect();
    const style = getComputedStyle(canvas);
    const corners = ['borderTopLeftRadius', 'borderTopRightRadius', 'borderBottomLeftRadius', 'borderBottomRightRadius'];
    return {ratio:rect.width/rect.height, radii:corners.map(key => style[key].endsWith('%')
      ? parseFloat(style[key])/100 : parseFloat(style[key])/rect.width)};
  })()`);
  assert.ok(
    Math.abs(card.ratio - 1) < 0.005,
    'phone visualization matches the square native cover',
  );
  assert.ok(
    card.radii.every((radius) => Math.abs(radius - nativeRadius / nativeSide) < 0.001),
    'phone visualization preserves all four native cover corner radii',
  );
}

export async function verifyPhoneEffects({ call, evaluate, until, screenshot }, mode = 'desktop') {
  await assertNativePhoneCard(evaluate, '.hero-product__phone');
  if (mode === 'desktop') {
    assert.equal(
      await evaluate('matchMedia("(hover: hover)").matches'),
      true,
      'hover-capable device',
    );
    const hover = async (selector) => {
      const point = await evaluate(`(() => {
        const r = document.querySelector('${selector}').getBoundingClientRect();
        return {x:r.x+r.width*.6,y:r.y+r.height*.4};
      })()`);
      await call('Input.dispatchMouseEvent', { type: 'mouseMoved', ...point });
      await delay(450);
      return evaluate(`getComputedStyle(document.querySelector('${selector}')).borderColor`);
    };
    const desktop = await hover('.hero-product__desktop');
    const phone = await hover('.hero-product__phone');
    assert.equal(phone, desktop, 'phone and desktop use the same hover edge');
    await call('Input.dispatchMouseEvent', { type: 'mouseMoved', x: 1, y: 1 });
  }
  const oil = (scope = '.hero-product__phone') =>
    evaluate(`(() => {
    const layer = document.querySelector('${scope} .phone-atmosphere__oil');
    return layer ? getComputedStyle(layer).transform : null;
  })()`);
  const first = await oil();
  assert.ok(first, 'phone has a moving oil atmosphere');
  if (mode === 'reduced') {
    await delay(350);
    assert.equal(await oil(), first, 'reduced motion freezes the phone atmosphere');
  } else {
    await until(
      () => oil(),
      (value) => value !== first,
      'phone oil atmosphere keeps moving',
    );
  }
  if (mode !== 'desktop') return;
  await evaluate('document.querySelector("#film").scrollIntoView({behavior:"instant"})');
  await delay(250);
  const hidden = await oil();
  await delay(350);
  assert.equal(await oil(), hidden, 'offscreen phone atmosphere stops');
  await evaluate('window.scrollTo({top:0,behavior:"instant"})');
  await until(
    () => oil(),
    (value) => value !== hidden,
    'phone atmosphere resumes',
  );
  await evaluate('document.querySelector(".hero-product__phone").click()');
  await until(() => oil('.lightbox'), Boolean, 'enlarged phone atmosphere exists');
  await assertNativePhoneCard(evaluate, '.lightbox');
  const enlarged = await oil('.lightbox');
  await until(
    () => oil('.lightbox'),
    (value) => value !== enlarged,
    'enlarged phone atmosphere moves',
  );
  const covered = await oil();
  await delay(350);
  assert.equal(await oil(), covered, 'covered phone atmosphere stops');
  await screenshot('phone-oil-lightbox.png');
  await evaluate(
    'document.querySelector(".lightbox").dispatchEvent(new KeyboardEvent("keydown", {key:"Escape",bubbles:true}))',
  );
  await until(
    () => evaluate('!document.querySelector(".lightbox")'),
    Boolean,
    'phone lightbox closed',
  );
  console.log('PASS: matching hover edges and phone oil motion, enlargement and pauses');
}
