# HDR pipeline & bloom — design plan

Drafted and **IMPLEMENTED** 2026-07-20. Follow-up to the lighting stack
(directional sun, RGB flood-fill, per-light shadows, stained beams). Goal:
give fire real overbright energy and let it **glow**, without muddying the
crisp reads the rest of the lighting works to earn.

## Why the direct-to-sRGB path couldn't do this

Every scene pass wrote straight to the `Rgba8UnormSrgb` swapchain, which
clamps at 1.0. A torch flame, a lit bloomery, the sun glinting off water —
all clipped flat at white. There was no headroom, so there was nothing for a
glow to key off: a threshold bloom would either catch nothing (threshold at
1.0) or catch every bright-but-ordinary surface (threshold below 1.0) and
smear the whole frame. The fix is to render the world into a **linear HDR
target** first, keep values above 1.0, and tonemap on the way out.

## The pipeline

1. **HDR scene target** — the world (opaque, water, entities, outline, and
   the first-person hand) renders into an `Rgba16Float` buffer at full
   resolution instead of the swapchain. Values are unclamped and linear.
2. **Bright pass** — a fullscreen pass isolates the energy above a 1.0 knee
   (soft, so the mask has no hard edge), writing to a **half-resolution**
   bloom buffer (cheaper, and a wider effective blur).
3. **Separable blur** — a 9-tap Gaussian horizontally then vertically,
   ping-ponging between two half-res buffers.
4. **Composite** — `scene + intensity·bloom`, clamped, written to the sRGB
   swapchain (which encodes it exactly as the direct pass used to). With
   bloom off (intensity 0) the image is byte-for-byte the old render.
5. **UI** — the crosshair and 2D batch draw last, straight to the swapchain,
   so the HUD never blooms or tonemaps.

The four fullscreen passes are size-independent pipelines; the targets and
their bind groups rebuild on resize. All of it lives in `renderer.rs` +
`post.wgsl`.

## Making emitters glow (the emissive trick)

Bloom is only as good as what exceeds 1.0. Rather than add a vertex
attribute (16 construction sites across 7 files) and a new suppression-aware
term in the shader, we reuse the channel that already means *self-lit block
color*: an emitter's own faces have their block-light overwritten in the
mesher with the block's emission color at an overbright **gain** (~3.5×), so
the tile reads well past white even after point-light suppression and face
shading subtract from it. Ordinary faces are untouched — the glow is the
emitter's alone, and the bloom blur is what bleeds it onto neighbours in
screen space (real firelight, not a painted-on ring).

## Taste

The default keys strictly off the 1.0 knee, so lit stone next to a torch does
*not* bloom — only the flame does. Intensity ships **strong** (1.5): a
glaring firelight halo, per the project's high-dynamic-range aesthetic, while
the flame's pixel core stays sharp. `BLOOM` is its own settings toggle
(persisted to `config.txt`), off restoring the exact pre-bloom image.

## Tests

- `wgsl_shaders_validate` parses and validates both `shader.wgsl` and
  `post.wgsl` through naga at test time — a shader typo now fails CI instead
  of a black screen on someone's GPU (the shaders were previously only
  compiled at device init).
- The existing suites stay green; the composite is identity when bloom is
  off, so nothing sim-facing or UI-facing moved.

## What this deliberately does not do

No global filmic tonemap curve — the composite clamps, preserving the punchy
mid-range the lighting already produces (a heavy curve would soften exactly
the contrast we want). No exposure/eye-adaptation. No god-rays yet — but the
HDR buffer this adds is the thing volumetric shafts would reuse.
