//! A pot of pink pixel-art flowers grown from a [`BloomDevelopment`], drawn frame by frame.
//!
//! The pot never changes. The commit total sets the stage and size: a seed, a sprout
//! with two seed leaves, a bud, then an open pink flower that keeps growing with
//! diminishing gains. More flowers join as commits pile up, each arriving as a bud
//! and opening later. Every flower's center takes one of the repository's top
//! languages' colors, largest language on the main flower. The running weekly streak
//! adds leaves, and the longest streak makes the petals more vivid and, past half a
//! year, makes them glint. The mood sets the motion of one seamless loop. Identical
//! inputs give identical pixels without I/O or clock reads.

use std::f64::consts::{PI, TAU};

use crate::growth::{
    BLOOM_COMMITS, BUD_COMMITS, BloomDevelopment, FULL_COLOR_COMMITS, Stage, growth_fraction,
};
use crate::mood::Mood;

/// Width and height of the native canvas, in pixels.
pub const CANVAS_PIXELS: u32 = 128;
/// Frames per second of every loop.
pub const FRAMES_PER_SECOND: u32 = 15;
/// Commit totals at which the second, third, fourth, and fifth flowers appear as buds.
/// Each opens once the total reaches half again its threshold.
pub const EXTRA_FLOWER_COMMITS: [u64; 4] = [150, 600, 2_000, 6_000];

const CANVAS: usize = CANVAS_PIXELS as usize;
const POT_CENTER_X: f64 = 64.0;
const SOIL_Y: f64 = 106.0;
const FIT_MARGIN: f64 = 3.0;
const EXTRA_FLOWER_OPENING: f64 = 1.5;

type Rgb = [u8; 3];
type Ramp = [Rgb; 4];

const OUTLINE: [u8; 4] = [33, 24, 44, 255];
const GREEN_RAMP: Ramp = [[28, 66, 58], [44, 108, 66], [86, 158, 74], [156, 206, 100]];
const DRY_RAMP: Ramp = [[72, 60, 44], [112, 94, 56], [150, 128, 74], [186, 166, 104]];
const POT_RAMP: Ramp = [
    [104, 48, 40],
    [156, 74, 50],
    [198, 108, 68],
    [228, 152, 104],
];
const SOIL_RAMP: Ramp = [[46, 30, 30], [70, 46, 38], [96, 66, 48], [122, 88, 60]];
const GOLD: Rgb = [244, 186, 64];
const BROWN_RAMP: Ramp = [[52, 32, 28], [88, 54, 36], [128, 84, 50], [170, 120, 70]];
/// The petal pinks, main flower first; extra flowers cycle through the others.
const PINKS: [Rgb; 3] = [[240, 104, 156], [248, 146, 186], [226, 84, 136]];
const WITHER_BROWN: [f64; 3] = [146.0, 102.0, 64.0];
const POLLEN: [u8; 4] = [255, 226, 120, 255];
const GLINT: [u8; 4] = [255, 255, 255, 255];
const DITHER_RANKS: [[u8; 4]; 4] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];

/// One frame of row-major RGBA pixels on the native canvas. Every pixel is either
/// fully transparent or fully opaque.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BloomFrame {
    rgba_pixels: Vec<[u8; 4]>,
}

impl BloomFrame {
    /// The pixels as bytes, row by row from the top left, four channels per pixel.
    pub fn as_rgba_bytes(&self) -> &[u8] {
        self.rgba_pixels.as_flattened()
    }

    /// The pixel at a position on the native canvas.
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        self.rgba_pixels[(y * CANVAS_PIXELS + x) as usize]
    }
}

/// Frames in one seamless loop of a mood.
pub fn frame_count(mood: Mood) -> u32 {
    match mood {
        Mood::Sway | Mood::Wilt => 120,
        Mood::Burst | Mood::Unfold | Mood::Rest => 90,
    }
}

/// How many flowers a commit total grows, one through five.
pub fn flower_count(total_commits: u64) -> usize {
    1 + EXTRA_FLOWER_COMMITS
        .iter()
        .filter(|&&threshold| total_commits >= threshold)
        .count()
}

/// Draw one frame of the mood's loop; frame numbers wrap around the loop. Frame zero
/// makes a good still.
pub fn render_frame(development: &BloomDevelopment, mood: Mood, frame_index: u32) -> BloomFrame {
    let garden = Garden::new(development);
    let fit = fit_loop(&garden, mood);
    let frame_index = frame_index % frame_count(mood);
    let mut canvas = Canvas::new();
    let scenes: Vec<(usize, Pose, Scene)> = garden
        .drawing_order()
        .into_iter()
        .map(|index| {
            let flower = &garden.flowers[index];
            let pose = Pose::new(mood, frame_index, flower.phase_frames);
            let scene = Scene::build(flower, &pose, fit);
            (index, pose, scene)
        })
        .collect();
    for (index, pose, scene) in &scenes {
        let flower = &garden.flowers[*index];
        paint_stem(&mut canvas, flower, pose, scene);
        for leaf in &scene.leaves {
            paint_leaf(&mut canvas, leaf, pose);
        }
        match flower.stage {
            Stage::Seed | Stage::Sprout => {}
            Stage::Bud => paint_bud(&mut canvas, &garden, flower, pose, scene),
            Stage::Bloom => paint_flower(&mut canvas, &garden, flower, pose, scene),
        }
    }
    paint_pot(
        &mut canvas,
        fit.shift,
        garden.flowers[0].stage == Stage::Seed,
    );
    if let Some((_, pose, scene)) = scenes.iter().find(|(index, _, _)| *index == 0) {
        paint_effects(&mut canvas, &garden, &garden.flowers[0], pose, scene);
    }
    canvas.outline();
    BloomFrame {
        rgba_pixels: canvas.pixels,
    }
}

/// What the development fixes for every flower and frame.
struct Garden {
    seed: u64,
    flowers: Vec<Flower>,
    language_colors: Vec<Option<Rgb>>,
    vividness: f64,
    paleness: f64,
    glint: f64,
}

/// One flower in the pot; the first is the main one, in the middle and in front.
struct Flower {
    index: usize,
    stage: Stage,
    stem_height: f64,
    stem_half_width: f64,
    head_radius: f64,
    bud_progress: f64,
    leaf_length: f64,
    leaf_count: usize,
    first_leaf_side: f64,
    lean: f64,
    base_offset: f64,
    droop_side: f64,
    petal_phase: f64,
    petal_count: usize,
    pink: Rgb,
    /// Loop frames this flower runs ahead, so the flowers do not move in lockstep.
    phase_frames: u32,
}

impl Garden {
    fn new(development: &BloomDevelopment) -> Self {
        let total = development.total_commits;
        let seed = development.identity_seed;
        let growth = growth_fraction(total);
        let mirror = if unit(seed, 2) < 0.5 { -1.0 } else { 1.0 };
        let main_stage = development.stage();
        let main_height = match main_stage {
            Stage::Seed => 0.0,
            _ => lerp(9.0, 64.0, growth),
        };
        let bloom_growth = growth_fraction(BLOOM_COMMITS);
        let bud_growth = growth_fraction(BUD_COMMITS);
        let main_progress =
            smooth_step(((growth - bloom_growth) / (1.0 - bloom_growth)).clamp(0.0, 1.0));
        let streak_leaves = (development.current_streak_weeks / 2).min(4) as usize;
        let mut flowers = Vec::new();
        for index in 0..flower_count(total) {
            let (stage, progress, bud_progress, height) = if index == 0 {
                (
                    main_stage,
                    main_progress,
                    ((growth - bud_growth) / (bloom_growth - bud_growth)).clamp(0.0, 1.0),
                    main_height,
                )
            } else {
                let threshold = EXTRA_FLOWER_COMMITS[index - 1];
                let opening = (threshold as f64 * EXTRA_FLOWER_OPENING) as u64;
                let start = growth_fraction(threshold);
                let open = growth_fraction(opening);
                let progress = ((growth - open) / (1.0 - open)).clamp(0.0, 1.0);
                let stage = if total >= opening {
                    Stage::Bloom
                } else {
                    Stage::Bud
                };
                let bud_progress = ((growth - start) / (open - start)).clamp(0.0, 1.0);
                let height = main_height * lerp(0.5, 0.7, (growth - start) / (1.0 - start))
                    - 7.0 * ((index - 1) / 2) as f64;
                (stage, smooth_step(progress), bud_progress, height)
            };
            // Extra flowers alternate sides, the later pair further out.
            let side = if index == 0 {
                0.0
            } else if index % 2 == 1 {
                mirror
            } else {
                -mirror
            };
            let outer = if index >= 3 { 1.0 } else { 0.0 };
            let main_scale = if index == 0 { 1.0 } else { 0.62 };
            let leaf_count = match stage {
                Stage::Seed => 0,
                Stage::Sprout => 2,
                Stage::Bud if index == 0 => 1 + streak_leaves.min(1),
                Stage::Bud => 1,
                Stage::Bloom if index == 0 => 2 + streak_leaves,
                Stage::Bloom => 1 + streak_leaves / 2,
            }
            .min(((height / 9.0) as usize).max(if index == 0 { 2 } else { 1 }));
            flowers.push(Flower {
                index,
                stage,
                stem_height: height,
                stem_half_width: lerp(0.8, 1.7, growth) * if index == 0 { 1.0 } else { 0.8 },
                head_radius: lerp(9.0, 17.0, progress) * main_scale,
                bud_progress,
                leaf_length: lerp(5.0, 15.0, growth) * if index == 0 { 1.0 } else { 0.75 },
                leaf_count,
                first_leaf_side: if unit(seed, 10 + index as u64) < 0.5 {
                    -1.0
                } else {
                    1.0
                },
                lean: if index == 0 {
                    (unit(seed, 1) - 0.5) * 0.12
                } else {
                    side * (0.58 + 0.22 * outer + (unit(seed, 20 + index as u64) - 0.5) * 0.08)
                },
                base_offset: side * (4.0 + 3.0 * outer),
                droop_side: if index == 0 { mirror } else { side },
                petal_phase: unit(seed, 3 + 100 * index as u64) * TAU,
                petal_count: 5 + (progress * 3.0).round() as usize + ((seed >> index) % 2) as usize,
                pink: PINKS[if index == 0 { 0 } else { 1 + (index - 1) % 2 }],
                phase_frames: (unit(seed, 30 + index as u64) * 40.0) as u32 * u32::from(index > 0),
            });
        }
        let longest = f64::from(development.longest_streak_weeks);
        Self {
            seed,
            flowers,
            language_colors: development
                .languages
                .iter()
                .map(|language| language.color)
                .collect(),
            vividness: if longest < 4.0 {
                0.75
            } else {
                lerp(0.92, 1.2, ((longest - 4.0) / 48.0).min(1.0))
            },
            paleness: (1.0 - total as f64 / FULL_COLOR_COMMITS as f64).max(0.0) * 0.5,
            glint: ((longest - 26.0) / 78.0).clamp(0.0, 1.0),
        }
    }

    /// Outer flowers first, the main flower last, so nearer flowers cover farther ones.
    fn drawing_order(&self) -> Vec<usize> {
        let mut order: Vec<usize> = (0..self.flowers.len()).collect();
        order.sort_by_key(|&index| match index {
            0 => 9,
            index => 5 - index,
        });
        order
    }

    fn color_ramp(&self, color: Rgb, pose: &Pose) -> Ramp {
        let mut channels = color.map(f64::from);
        channels = saturate(channels, self.vividness * (1.0 - 0.45 * pose.wither));
        let luminance = luminance(channels);
        if luminance < 100.0 {
            channels = mix(
                channels,
                [255.0; 3],
                (100.0 - luminance) / (255.0 - luminance),
            );
        }
        channels = mix(channels, [255.0; 3], self.paleness);
        channels = mix(channels, WITHER_BROWN, pose.wither * 0.45);
        build_ramp(channels)
    }

    fn petal_ramp(&self, flower: &Flower, pose: &Pose) -> Ramp {
        self.color_ramp(flower.pink, pose)
    }

    /// The flower's center: one of the top languages, the largest on the main flower.
    fn center_ramp(&self, flower: &Flower, pose: &Pose) -> Ramp {
        let color = match self.language_colors.len() {
            0 => GOLD,
            count => self.language_colors[flower.index % count].unwrap_or(GOLD),
        };
        blend_ramps(
            &self.color_ramp(color, pose),
            &BROWN_RAMP,
            pose.wither * 0.5,
        )
    }
}

/// What the mood sets for one frame of one flower.
struct Pose {
    mood: Mood,
    /// Loop position from zero to one.
    time: f64,
    sway: f64,
    droop: f64,
    openness: f64,
    wither: f64,
    spin: f64,
    flutter: f64,
}

impl Pose {
    fn new(mood: Mood, frame_index: u32, phase_frames: u32) -> Self {
        let frames = frame_count(mood);
        let time = f64::from(frame_index % frames) / f64::from(frames);
        let shifted = f64::from((frame_index + phase_frames) % frames) / f64::from(frames);
        let wave = |cycles: f64| (TAU * cycles * shifted).sin();
        let (sway, droop, openness, wither, spin) = match mood {
            Mood::Sway => (0.06 * wave(2.0), 0.0, 1.0, 0.0, 0.0),
            Mood::Burst => (0.10 * wave(3.0), 0.0, 1.0, 0.0, 0.10 * wave(1.0)),
            Mood::Unfold => (
                0.03 * wave(2.0),
                0.0,
                0.62 + 0.38 * (0.5 + 0.5 * (TAU * shifted).cos()),
                0.0,
                0.0,
            ),
            Mood::Rest => (0.025 * wave(1.0), 0.5, 0.74, 0.25, 0.0),
            Mood::Wilt => (0.012 * wave(1.0), 0.9, 0.6, 0.75, 0.0),
        };
        Self {
            mood,
            time,
            sway,
            droop,
            openness,
            wither,
            spin,
            flutter: wave(4.0),
        }
    }
}

struct Leaf {
    root: [f64; 2],
    angle: f64,
    length: f64,
    width: f64,
    round: bool,
}

/// Where one flower is in one frame.
struct Scene {
    stem: Vec<[f64; 2]>,
    head_center: [f64; 2],
    head_angle: f64,
    squash: f64,
    leaves: Vec<Leaf>,
}

/// How the whole loop is placed: one horizontal shift, and how far the side flowers
/// fan out, so every frame fits.
#[derive(Clone, Copy)]
struct Fit {
    shift: f64,
    spread: f64,
}

impl Scene {
    fn build(flower: &Flower, pose: &Pose, fit: Fit) -> Self {
        const SEGMENTS: usize = 40;
        let angle_at = |along: f64| {
            flower.lean * fit.spread * (0.6 + 0.4 * along)
                + pose.sway * along.powf(1.5)
                + flower.droop_side * pose.droop * along.powf(2.5)
        };
        let step = flower.stem_height / SEGMENTS as f64;
        let mut stem = vec![[
            POT_CENTER_X + fit.shift + flower.base_offset * fit.spread,
            SOIL_Y + 2.0,
        ]];
        for segment in 0..SEGMENTS {
            let angle = angle_at((segment as f64 + 0.5) / SEGMENTS as f64);
            let [x, y] = stem[segment];
            stem.push([x + angle.sin() * step, y - angle.cos() * step]);
        }
        let head_angle = angle_at(1.0);
        let head_center = *stem.last().unwrap();
        let mut leaves = Vec::new();
        if flower.stage == Stage::Sprout {
            for side in [-1.0, 1.0] {
                leaves.push(Leaf {
                    root: head_center,
                    angle: head_angle + side * (1.05 + 0.08 * pose.flutter + 0.4 * pose.wither),
                    length: lerp(3.0, 6.0, flower.stem_height / 20.0),
                    width: lerp(1.8, 3.4, flower.stem_height / 20.0),
                    round: true,
                });
            }
        } else {
            for index in 0..flower.leaf_count {
                let along = 0.16 + 0.56 * (index as f64 + 0.5) / flower.leaf_count as f64;
                let side = if index % 2 == 0 {
                    flower.first_leaf_side
                } else {
                    -flower.first_leaf_side
                };
                let point = (along * SEGMENTS as f64).round() as usize;
                let length = flower.leaf_length * (1.0 - 0.3 * along);
                leaves.push(Leaf {
                    root: stem[point],
                    angle: angle_at(along)
                        + side * (0.95 - 0.15 * along + 0.07 * pose.flutter + 0.55 * pose.wither),
                    length,
                    width: length * 0.36,
                    round: false,
                });
            }
        }
        Self {
            stem,
            head_center,
            head_angle,
            squash: 1.0 - 0.38 * pose.droop.min(1.0),
            leaves,
        }
    }

    /// The leftmost and rightmost columns this flower can reach in this frame.
    fn horizontal_extent(&self, flower: &Flower) -> [f64; 2] {
        let mut extent = [f64::INFINITY, f64::NEG_INFINITY];
        let mut include = |x: f64| {
            extent[0] = extent[0].min(x);
            extent[1] = extent[1].max(x);
        };
        let head_reach = match flower.stage {
            Stage::Bloom => flower.head_radius * 1.1,
            Stage::Bud => 7.0,
            _ => 0.0,
        };
        include(self.head_center[0] - head_reach);
        include(self.head_center[0] + head_reach);
        for leaf in &self.leaves {
            include(leaf.root[0] + leaf.angle.sin() * leaf.length);
        }
        extent
    }
}

/// Choose one placement for the whole loop: fan the side flowers out as far as fits,
/// then shift everything sideways to keep every frame inside the canvas.
fn fit_loop(garden: &Garden, mood: Mood) -> Fit {
    let room = CANVAS as f64 - 2.0 * FIT_MARGIN;
    let mut fallback = Fit {
        shift: 0.0,
        spread: 0.3,
    };
    for step in 0..8 {
        let spread = 1.0 - 0.1 * step as f64;
        let trial = Fit { shift: 0.0, spread };
        let mut extent = [POT_CENTER_X - 19.0, POT_CENTER_X + 19.0];
        for frame_index in 0..frame_count(mood) {
            for flower in &garden.flowers {
                let pose = Pose::new(mood, frame_index, flower.phase_frames);
                let [left, right] = Scene::build(flower, &pose, trial).horizontal_extent(flower);
                extent = [extent[0].min(left), extent[1].max(right)];
            }
        }
        let shift = if extent[1] > FIT_MARGIN + room {
            FIT_MARGIN + room - extent[1]
        } else if extent[0] < FIT_MARGIN {
            FIT_MARGIN - extent[0]
        } else {
            0.0
        };
        fallback = Fit { shift, spread };
        if extent[1] - extent[0] <= room {
            return fallback;
        }
    }
    fallback
}

struct Canvas {
    pixels: Vec<[u8; 4]>,
}

impl Canvas {
    fn new() -> Self {
        Self {
            pixels: vec![[0; 4]; CANVAS * CANVAS],
        }
    }

    fn set(&mut self, x: i64, y: i64, color: [u8; 4]) {
        if (0..CANVAS as i64).contains(&x) && (0..CANVAS as i64).contains(&y) {
            self.pixels[y as usize * CANVAS + x as usize] = color;
        }
    }

    /// Visit every pixel center inside a box, clipped to the canvas.
    fn each_in(
        &mut self,
        min: [f64; 2],
        max: [f64; 2],
        mut visit: impl FnMut(&mut Self, i64, i64),
    ) {
        let low = |value: f64| (value.floor() as i64).clamp(0, CANVAS as i64);
        let high = |value: f64| (value.ceil() as i64 + 1).clamp(0, CANVAS as i64);
        for y in low(min[1])..high(max[1]) {
            for x in low(min[0])..high(max[0]) {
                visit(self, x, y);
            }
        }
    }

    /// Draw a dark outline around everything drawn.
    fn outline(&mut self) {
        let drawn: Vec<bool> = self.pixels.iter().map(|pixel| pixel[3] != 0).collect();
        for y in 0..CANVAS {
            for x in 0..CANVAS {
                if drawn[y * CANVAS + x] {
                    continue;
                }
                let touches = [(0, -1), (0, 1), (-1, 0), (1, 0)].iter().any(|(dx, dy)| {
                    let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                    (0..CANVAS as i64).contains(&nx)
                        && (0..CANVAS as i64).contains(&ny)
                        && drawn[ny as usize * CANVAS + nx as usize]
                });
                if touches {
                    self.pixels[y * CANVAS + x] = OUTLINE;
                }
            }
        }
    }
}

fn paint_stem(canvas: &mut Canvas, flower: &Flower, pose: &Pose, scene: &Scene) {
    if flower.stage == Stage::Seed {
        return;
    }
    let ramp = blend_ramps(&GREEN_RAMP, &DRY_RAMP, pose.wither);
    let half_width = flower.stem_half_width;
    let (min, max) = bounds(&scene.stem, half_width + 1.0);
    canvas.each_in(min, max, |canvas, x, y| {
        let point = [x as f64 + 0.5, y as f64 + 0.5];
        let mut best: Option<(f64, f64)> = None;
        for segment in scene.stem.windows(2) {
            let (distance, side) = distance_to_segment(point, segment[0], segment[1]);
            if best.is_none_or(|(best_distance, _)| distance < best_distance) {
                best = Some((distance, side));
            }
        }
        if let Some((distance, side)) = best
            && distance <= half_width
        {
            let brightness = 0.55 - 0.4 * side / half_width.max(1.0);
            canvas.set(x, y, shade(&ramp, brightness, x, y));
        }
    });
}

fn paint_pot(canvas: &mut Canvas, shift: f64, seed_only: bool) {
    let center = POT_CENTER_X + shift;
    canvas.each_in(
        [center - 19.0, SOIL_Y - 3.0],
        [center + 19.0, 125.0],
        |canvas, x, y| {
            let px = x as f64 + 0.5 - center;
            let row = y as f64;
            let (half_width, ramp, base) = if row < SOIL_Y {
                (
                    lerp(12.0, 15.5, (row - (SOIL_Y - 2.0)) / 2.0),
                    &SOIL_RAMP,
                    0.45,
                )
            } else if row < SOIL_Y + 5.0 {
                (18.0, &POT_RAMP, if row < SOIL_Y + 1.0 { 0.95 } else { 0.7 })
            } else if row < 124.0 {
                let fall = (row - (SOIL_Y + 5.0)) / (124.0 - (SOIL_Y + 5.0));
                let under_rim = if row < SOIL_Y + 6.0 { -0.35 } else { 0.0 };
                (lerp(15.0, 11.5, fall), &POT_RAMP, 0.6 + under_rim)
            } else {
                return;
            };
            if row < SOIL_Y - 2.0 || px.abs() > half_width {
                return;
            }
            let brightness = base - 0.5 * px / half_width;
            canvas.set(x, y, shade_with(ramp, brightness, x, y, 0.6));
        },
    );
    if seed_only {
        for (dx, dy, index) in [(-1, -3, 2), (0, -3, 3), (-1, -2, 1), (0, -2, 2)] {
            canvas.set(
                center as i64 + dx,
                SOIL_Y as i64 + dy,
                opaque(BROWN_RAMP[index]),
            );
        }
    }
}

fn paint_leaf(canvas: &mut Canvas, leaf: &Leaf, pose: &Pose) {
    let ramp = blend_ramps(&GREEN_RAMP, &DRY_RAMP, pose.wither);
    let direction = [leaf.angle.sin(), -leaf.angle.cos()];
    let normal = [-direction[1], direction[0]];
    let upward_side = if normal[1] < 0.0 { 1.0 } else { -1.0 };
    let tip = [
        leaf.root[0] + direction[0] * leaf.length,
        leaf.root[1] + direction[1] * leaf.length,
    ];
    let (min, max) = bounds(&[leaf.root, tip], leaf.width + 1.0);
    canvas.each_in(min, max, |canvas, x, y| {
        let offset = [x as f64 + 0.5 - leaf.root[0], y as f64 + 0.5 - leaf.root[1]];
        let along = dot(offset, direction) / leaf.length;
        let across = dot(offset, normal);
        if !(0.0..=1.0).contains(&along) {
            return;
        }
        let profile = if leaf.round {
            (PI * along).sin().powf(0.5)
        } else {
            (PI * along).sin().powf(0.7) * (1.0 - 0.2 * along)
        };
        let half_width = leaf.width * profile;
        if across.abs() > half_width {
            return;
        }
        let mut brightness = 0.5 + 0.22 * upward_side * across.signum() + 0.12 * along;
        if !leaf.round && across.abs() < 0.55 && (0.1..0.85).contains(&along) {
            brightness = 0.22;
        }
        if across.abs() > half_width * 0.82 {
            brightness -= 0.2;
        }
        canvas.set(x, y, shade_with(&ramp, brightness, x, y, 0.6));
    });
}

/// Pixel offsets in the head's frame: `[across, up]`, with `up` unsquashed.
fn head_local(scene: &Scene, x: i64, y: i64) -> [f64; 2] {
    let offset = [
        x as f64 + 0.5 - scene.head_center[0],
        y as f64 + 0.5 - scene.head_center[1],
    ];
    let up = [scene.head_angle.sin(), -scene.head_angle.cos()];
    let right = [-up[1], up[0]];
    [dot(offset, right), dot(offset, up) / scene.squash]
}

fn head_world(scene: &Scene, local: [f64; 2]) -> [f64; 2] {
    let up = [scene.head_angle.sin(), -scene.head_angle.cos()];
    let right = [-up[1], up[0]];
    let up_amount = local[1] * scene.squash;
    [
        scene.head_center[0] + right[0] * local[0] + up[0] * up_amount,
        scene.head_center[1] + right[1] * local[0] + up[1] * up_amount,
    ]
}

fn paint_bud(canvas: &mut Canvas, garden: &Garden, flower: &Flower, pose: &Pose, scene: &Scene) {
    let radius = lerp(2.6, 5.2, flower.bud_progress);
    let sepal_ramp = blend_ramps(&GREEN_RAMP, &DRY_RAMP, pose.wither);
    let petal_ramp = garden.petal_ramp(flower, pose);
    let reach = radius * 2.6;
    let center = scene.head_center;
    canvas.each_in(
        [center[0] - reach, center[1] - reach],
        [center[0] + reach, center[1] + reach],
        |canvas, x, y| {
            let [across, up] = head_local(scene, x, y);
            let along = (up + 0.4 * radius) / (2.4 * radius);
            if !(0.0..=1.0).contains(&along) {
                return;
            }
            let half_width = radius * (PI * along.powf(0.75)).sin();
            if across.abs() > half_width {
                return;
            }
            let sepal_line = 0.62 - 0.3 * flower.bud_progress + 0.2 * across.abs() / radius;
            let brightness = 0.55 - 0.35 * across / radius.max(1.0) + 0.15 * along;
            let ramp = if along < sepal_line {
                &sepal_ramp
            } else {
                &petal_ramp
            };
            canvas.set(x, y, shade(ramp, brightness, x, y));
        },
    );
}

struct PetalHit {
    distance: f64,
    along: f64,
    across: f64,
    petal: usize,
}

fn paint_flower(canvas: &mut Canvas, garden: &Garden, flower: &Flower, pose: &Pose, scene: &Scene) {
    let radius = flower.head_radius;
    let center_radius = (radius * 0.3).max(2.5);
    let petal_count = flower.petal_count;
    let petal_length = (radius - center_radius * 0.5) * pose.openness;
    let petal_ramp = garden.petal_ramp(flower, pose);
    let center_ramp = garden.center_ramp(flower, pose);
    let slot = TAU / petal_count as f64;
    let middle = center_radius * 0.55 + petal_length / 2.0;
    let half_length = petal_length / 2.0 + center_radius * 0.45;
    let half_width = (half_length * 0.5).min(slot * (center_radius + petal_length * 0.55) * 0.56)
        * (0.7 + 0.3 * pose.openness);
    let light_angle = 0.75 * PI;
    let reach = radius * 1.15;
    let head = scene.head_center;
    canvas.each_in(
        [head[0] - reach, head[1] - reach],
        [head[0] + reach, head[1] + reach],
        |canvas, x, y| {
            let [across, up] = head_local(scene, x, y);
            let distance_out = across.hypot(up);
            let angle = up.atan2(across);
            if distance_out <= center_radius {
                let mut brightness = 0.55 + 0.35 * (up - across) / (2f64.sqrt() * center_radius);
                let cell = [(across * 0.9).floor() as i64, (up * 0.9).floor() as i64];
                if unit(
                    garden.seed ^ (cell[0] as u64).wrapping_mul(73_856_093),
                    cell[1] as u64,
                ) < 0.2
                {
                    brightness -= 0.35;
                }
                if distance_out > center_radius - 0.9 {
                    brightness = 0.05;
                }
                canvas.set(x, y, shade_with(&center_ramp, brightness, x, y, 0.5));
                return;
            }
            let ring_hit = |offset: f64, length_scale: f64| -> Option<PetalHit> {
                let mut best: Option<PetalHit> = None;
                for petal in 0..petal_count {
                    let petal_angle = flower.petal_phase + pose.spin + offset + petal as f64 * slot;
                    let delta = angle - petal_angle;
                    let lengthwise = distance_out * delta.cos() - middle * length_scale;
                    let sideways = distance_out * delta.sin();
                    let reach = half_length * length_scale;
                    let distance = (lengthwise / reach).powi(2) + (sideways / half_width).powi(2);
                    if distance <= 1.0 && best.as_ref().is_none_or(|hit| distance < hit.distance) {
                        best = Some(PetalHit {
                            distance,
                            along: (lengthwise + reach) / (2.0 * reach),
                            across: sideways / half_width,
                            petal,
                        });
                    }
                }
                best
            };
            let (hit, darkening, offset) = match ring_hit(0.0, 1.0) {
                Some(hit) => (hit, 0.0, 0.0),
                None => match ring_hit(slot / 2.0, 1.06) {
                    Some(hit) => (hit, 0.28, slot / 2.0),
                    None => return,
                },
            };
            let petal_angle = flower.petal_phase + pose.spin + offset + hit.petal as f64 * slot;
            let mut brightness = 0.5 + 0.2 * hit.along - 0.1 * hit.across.abs()
                + 0.12 * (petal_angle - light_angle).cos()
                - darkening;
            if hit.along < 0.3 {
                brightness -= 0.15;
            }
            if hit.across.abs() < 0.14 && (0.2..0.7).contains(&hit.along) {
                brightness -= 0.12;
            }
            if hit.distance > 0.78 {
                brightness = 0.02;
            }
            canvas.set(x, y, shade_with(&petal_ramp, brightness, x, y, 0.4));
        },
    );
}

/// Effects belong to the main flower: glints, pollen, sun sparkles, a falling petal.
fn paint_effects(
    canvas: &mut Canvas,
    garden: &Garden,
    flower: &Flower,
    pose: &Pose,
    scene: &Scene,
) {
    let head = scene.head_center;
    let radius = flower.head_radius;
    let seed = garden.seed;
    if flower.stage == Stage::Bloom && garden.glint > 0.0 {
        let petal_count = flower.petal_count;
        let petal_length = (radius - radius * 0.15) * pose.openness;
        for petal in 0..petal_count {
            if unit(seed, 100 + petal as u64) > garden.glint * 0.7 + 0.15 {
                continue;
            }
            let twinkle = (TAU * (2.0 * pose.time + unit(seed, 200 + petal as u64))).sin();
            if twinkle < -0.2 {
                continue;
            }
            let angle = flower.petal_phase + pose.spin + petal as f64 * TAU / petal_count as f64;
            let out = radius * 0.3 + petal_length * 0.6;
            let [x, y] = head_world(scene, [out * angle.cos(), out * angle.sin()]);
            canvas.set(x as i64, y as i64, GLINT);
        }
    }
    match pose.mood {
        Mood::Burst if flower.stage == Stage::Bloom => {
            for grain in 0..14u64 {
                let rise = (2.0 * pose.time + grain as f64 / 14.0).fract();
                if rise > 0.9 {
                    continue;
                }
                let start = [
                    head[0] + (unit(seed, 300 + grain) - 0.5) * radius * 0.5,
                    head[1] + (unit(seed, 400 + grain) - 0.5) * radius * 0.4,
                ];
                let drift = (unit(seed, 500 + grain) - 0.5) * 30.0;
                let x = start[0] + drift * rise + (TAU * (rise + grain as f64 * 0.3)).sin() * 2.0;
                let y = start[1] - rise * (start[1] - FIT_MARGIN - 1.0).min(40.0);
                canvas.set(
                    x as i64,
                    y as i64,
                    if grain % 3 == 0 { GLINT } else { POLLEN },
                );
                if grain % 4 == 1 && rise < 0.5 {
                    canvas.set(x as i64 + 1, y as i64, POLLEN);
                }
            }
        }
        Mood::Unfold if flower.stage >= Stage::Bud => {
            let ring = if flower.stage == Stage::Bloom {
                radius * 1.25 + 3.0
            } else {
                10.0
            };
            for spark in 0..3u64 {
                let angle =
                    -PI / 2.0 + (spark as f64 - 1.0) * 0.9 + (unit(seed, 600 + spark) - 0.5) * 0.4;
                let pulse = (TAU * (pose.time * 2.0 + spark as f64 / 3.0)).sin();
                if pulse < 0.0 {
                    continue;
                }
                let x = (head[0] + angle.cos() * ring) as i64;
                let y = (head[1] + angle.sin() * ring) as i64;
                canvas.set(x, y, GLINT);
                if pulse > 0.6 {
                    for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                        canvas.set(x + dx, y + dy, POLLEN);
                    }
                }
            }
        }
        Mood::Wilt if flower.stage == Stage::Bloom => {
            let fall = pose.time;
            if !(0.04..0.92).contains(&fall) {
                return;
            }
            let side = flower.droop_side;
            let start = [head[0] + side * radius * 0.5, head[1] + radius * 0.3];
            let end_y = SOIL_Y - 4.0;
            let position = [
                start[0] + side * 12.0 * fall + (TAU * 2.0 * fall).sin() * 3.0,
                lerp(start[1], end_y, fall),
            ];
            let ramp = garden.petal_ramp(flower, pose);
            let spin = TAU * 2.0 * fall;
            let axis = [spin.cos(), spin.sin()];
            canvas.each_in(
                [position[0] - 3.0, position[1] - 3.0],
                [position[0] + 3.0, position[1] + 3.0],
                |canvas, x, y| {
                    let offset = [x as f64 + 0.5 - position[0], y as f64 + 0.5 - position[1]];
                    let along = dot(offset, axis);
                    let across = dot(offset, [-axis[1], axis[0]]);
                    if (along / 2.6).powi(2) + (across / 1.4).powi(2) <= 1.0 {
                        canvas.set(x, y, shade(&ramp, 0.5 + 0.2 * along / 2.6, x, y));
                    }
                },
            );
        }
        _ => {}
    }
}

fn shade(ramp: &Ramp, brightness: f64, x: i64, y: i64) -> [u8; 4] {
    shade_with(ramp, brightness, x, y, 1.0)
}

/// Pick a ramp shade, dithering between neighbors. A `dither` below one keeps clean
/// bands and dithers only near band edges.
fn shade_with(ramp: &Ramp, brightness: f64, x: i64, y: i64, dither: f64) -> [u8; 4] {
    let level = brightness.clamp(0.0, 1.0) * 3.0;
    let base = level.floor();
    let rank = (f64::from(DITHER_RANKS[(y & 3) as usize][(x & 3) as usize]) + 0.5) / 16.0;
    let threshold = 0.5 + (rank - 0.5) * dither;
    let index = (base as usize + usize::from(level - base > threshold)).min(3);
    opaque(ramp[index])
}

fn opaque([red, green, blue]: Rgb) -> [u8; 4] {
    [red, green, blue, 255]
}

fn build_ramp(base: [f64; 3]) -> Ramp {
    let to_rgb =
        |channels: [f64; 3]| channels.map(|channel| channel.round().clamp(0.0, 255.0) as u8);
    [
        to_rgb(mix(base, [44.0, 26.0, 70.0], 0.55)),
        to_rgb(mix(base, [64.0, 40.0, 84.0], 0.22)),
        to_rgb(base),
        to_rgb(mix(base, [255.0, 246.0, 222.0], 0.42)),
    ]
}

fn blend_ramps(from: &Ramp, to: &Ramp, fraction: f64) -> Ramp {
    let mut ramp = *from;
    for (shade, target) in ramp.iter_mut().zip(to) {
        *shade = mix(shade.map(f64::from), target.map(f64::from), fraction)
            .map(|channel| channel.round() as u8);
    }
    ramp
}

fn mix(from: [f64; 3], to: [f64; 3], fraction: f64) -> [f64; 3] {
    [0, 1, 2].map(|index| from[index] + (to[index] - from[index]) * fraction)
}

fn luminance(channels: [f64; 3]) -> f64 {
    0.299 * channels[0] + 0.587 * channels[1] + 0.114 * channels[2]
}

fn saturate(channels: [f64; 3], factor: f64) -> [f64; 3] {
    let gray = luminance(channels);
    channels.map(|channel| (gray + (channel - gray) * factor).clamp(0.0, 255.0))
}

fn lerp(from: f64, to: f64, fraction: f64) -> f64 {
    from + (to - from) * fraction.clamp(0.0, 1.0)
}

fn smooth_step(fraction: f64) -> f64 {
    fraction * fraction * (3.0 - 2.0 * fraction)
}

fn dot(left: [f64; 2], right: [f64; 2]) -> f64 {
    left[0] * right[0] + left[1] * right[1]
}

fn bounds(points: &[[f64; 2]], padding: f64) -> ([f64; 2], [f64; 2]) {
    let mut min = [f64::INFINITY; 2];
    let mut max = [f64::NEG_INFINITY; 2];
    for point in points {
        for axis in 0..2 {
            min[axis] = min[axis].min(point[axis] - padding);
            max[axis] = max[axis].max(point[axis] + padding);
        }
    }
    (min, max)
}

/// Distance from a point to a segment, and how far right of the segment it lies.
fn distance_to_segment(point: [f64; 2], start: [f64; 2], end: [f64; 2]) -> (f64, f64) {
    let segment = [end[0] - start[0], end[1] - start[1]];
    let length_squared = dot(segment, segment).max(1e-9);
    let offset = [point[0] - start[0], point[1] - start[1]];
    let fraction = (dot(offset, segment) / length_squared).clamp(0.0, 1.0);
    let nearest = [
        start[0] + segment[0] * fraction,
        start[1] + segment[1] * fraction,
    ];
    let away = [point[0] - nearest[0], point[1] - nearest[1]];
    let length = length_squared.sqrt();
    let right = [-segment[1] / length, segment[0] / length];
    (dot(away, away).sqrt(), dot(offset, right))
}

/// A reproducible number in `[0, 1)` from the seed and a salt.
fn unit(seed: u64, salt: u64) -> f64 {
    (mix64(seed ^ mix64(salt)) >> 11) as f64 / (1u64 << 53) as f64
}

fn mix64(value: u64) -> u64 {
    let mut mixed = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    mixed ^ (mixed >> 31)
}
