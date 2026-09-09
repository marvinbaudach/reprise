import assert from 'node:assert/strict';
import { setTimeout as delay } from 'node:timers/promises';

export async function verifyDesktopScene({ evaluate, until, screenshot }, mode = 'full') {
  const frame = (scope = '.hero-product__desktop') =>
    evaluate(`document.querySelector('${scope} .desktop-scene canvas')?.toDataURL() ?? null`);
  const rotation = (scope = '.hero-product__desktop') =>
    evaluate(`(() => { const disc = document.querySelector('${scope} .desktop-scene__disc');
      return disc ? getComputedStyle(disc).transform : null; })()`);
  await until(() => frame(), Boolean, 'desktop visualization exists');
  if (mode === 'reduced') {
    const still = [await frame(), await rotation()];
    await delay(350);
    assert.deepEqual([await frame(), await rotation()], still, 'reduced motion freezes the scene');
    return;
  }
  const moving = async (scope) => {
    for (let sample = 0; sample < 2; sample += 1) {
      const first = await frame(scope);
      await until(
        () => frame(scope),
        (value) => value && value !== first,
        'desktop bars keep moving',
      );
    }
    const first = await rotation(scope);
    await until(
      () => rotation(scope),
      (value) => value && value !== first,
      'cover glow rotates',
    );
  };
  await moving();
  assert.ok(
    await evaluate(`(() => {
    const scene = document.querySelector('.hero-product__desktop .desktop-scene canvas').getBoundingClientRect();
    const picture = document.querySelector('.hero-product__desktop .shot-tile__picture').getBoundingClientRect();
    const phone = document.querySelector('.hero-product__phone').getBoundingClientRect();
    return scene.width > 0 && scene.height > 0 && scene.left > picture.left + picture.width * .8
      && scene.right <= picture.right + 1 && scene.bottom <= picture.bottom
      && (phone.right <= scene.left || phone.left >= scene.right || phone.top >= scene.bottom);
  })()`),
    'desktop visualization stays in its sidebar and clear of the phone',
  );
  if (mode !== 'full') return;
  await evaluate('document.querySelector("#film").scrollIntoView({behavior:"instant"})');
  await delay(250);
  const hidden = [await frame(), await rotation()];
  await delay(350);
  assert.deepEqual([await frame(), await rotation()], hidden, 'offscreen desktop scene stops');
  await evaluate('window.scrollTo({top:0,behavior:"instant"})');
  await moving();
  await evaluate('document.querySelector(".hero-product__desktop").click()');
  await until(() => frame('.lightbox'), Boolean, 'enlarged desktop scene exists');
  await moving('.lightbox');
  const covered = [await frame(), await rotation()];
  await delay(350);
  assert.deepEqual(
    [await frame(), await rotation()],
    covered,
    'covered hero stops behind the lightbox',
  );
  await screenshot('desktop-scene-lightbox.png');
  await evaluate(
    'document.querySelector(".lightbox").dispatchEvent(new KeyboardEvent("keydown", {key:"Escape",bubbles:true}))',
  );
  await until(() => evaluate('!document.querySelector(".lightbox")'), Boolean, 'lightbox closed');
  await moving();
  console.log('PASS: desktop bars and rotating cover glow move, enlarge and pause correctly');
}
