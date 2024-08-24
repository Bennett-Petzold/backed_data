use std::{
    io::{Cursor, Write},
    time::Duration,
};

use graphviz_rust::{cmd::Format, exec, parse, printer::PrinterContext};
use image::{
    codecs::gif::{GifEncoder, Repeat},
    Delay, Frame, ImageReader,
};
use rayon::iter::{ParallelBridge, ParallelIterator};

fn inject(line: &str, insert: &str) -> String {
    if let Some((before, after)) = line.rsplit_once(';') {
        before.to_string() + insert + ";" + after
    } else {
        line.to_string() + insert + ";"
    }
}

fn invis(line: &str) -> String {
    inject(line, "[ style = invis ]")
}

fn dashed(line: &str) -> String {
    inject(line, "[ style = dashed ]")
}

#[derive(Debug, Clone)]
pub enum InvisSequence<'a> {
    Base(&'a [usize]),
    NonBase(&'a [usize]),
}

impl AsRef<[usize]> for InvisSequence<'_> {
    fn as_ref(&self) -> &[usize] {
        match self {
            Self::Base(x) => x,
            Self::NonBase(x) => x,
        }
    }
}

impl From<InvisSequence<'_>> for Box<[usize]> {
    fn from(val: InvisSequence) -> Self {
        match val {
            InvisSequence::Base(x) => x.into(),
            InvisSequence::NonBase(x) => x.into(),
        }
    }
}

#[derive(Debug, Clone)]
struct AllSequence {
    invis: Box<[usize]>,
    dashed: Box<[usize]>,
    commented: Box<[usize]>,
}

impl AllSequence {
    /// Initial sequence, has no history so no dashed.
    fn invis(seq: InvisSequence, commented: &[usize]) -> Self {
        Self {
            invis: seq.into(),
            dashed: Box::new([]),
            commented: commented.into(),
        }
    }

    /// Dashes anything introduced in `prev`.
    fn pairs(
        prev: &InvisSequence,
        cur: InvisSequence,
        all_lines: &[usize],
        commented: &[usize],
    ) -> Self {
        // Reduce to all visible lines
        let mut dashed = all_lines.to_vec();
        dashed.retain(|x| !cur.as_ref().contains(x));

        // Reduce to all lines introduced in prev
        dashed.retain(|x| !prev.as_ref().contains(x));

        let mut commented: Box<[usize]> = commented.into();
        commented.sort_unstable();

        Self {
            invis: cur.into(),
            dashed: dashed.into_boxed_slice(),
            commented,
        }
    }

    // Output this line as a PNG via GraphViz
    fn render(&self, base: &[&str]) -> Vec<u8> {
        let mut base: Vec<String> = base.iter().map(|x| x.to_string()).collect();

        // Add invisible entries
        self.invis
            .iter()
            .for_each(|line| base[*line] = invis(&base[*line]));

        // Add dashed entries
        self.dashed.iter().for_each(|line| {
            // Only dash the lines with edges, not lines with boxes
            if base[*line].contains("->") {
                base[*line] = dashed(&base[*line])
            }
        });

        // Comment out entries
        self.commented.iter().rev().for_each(|line| {
            base.remove(*line);
        });

        let graph = parse(&base.into_iter().collect::<String>()).unwrap();
        exec(
            graph,
            &mut PrinterContext::default(),
            vec![Format::Gif.into()],
        )
        .unwrap()
    }
}

const FRAME_DELAY: Duration = Duration::from_millis(1_500);

pub fn render<W: Write>(
    base: &[&str],
    invis_sequences: &[InvisSequence],
    base_invis_seq: &[usize],
    comment_seq: &[&[usize]],
    target: W,
) {
    // All sequences minus the first
    let pairs = invis_sequences
        .iter()
        .zip(invis_sequences.iter().skip(1))
        .zip(comment_seq.iter().skip(1))
        .map(|((prev, cur), comment)| {
            AllSequence::pairs(prev, cur.clone(), base_invis_seq, comment)
        });

    // All sequences
    let resolved_sequences = [AllSequence::invis(
        invis_sequences[0].clone(),
        comment_seq[0],
    )]
    .into_iter()
    .chain(pairs);

    let mut encoder = GifEncoder::new(target);
    encoder.set_repeat(Repeat::Infinite).unwrap();

    let mut frames: Vec<_> = resolved_sequences
        .enumerate()
        .par_bridge()
        .map(|(idx, x)| {
            let rendered = x.render(base);
            let image = ImageReader::with_format(Cursor::new(rendered), image::ImageFormat::Gif)
                .decode()
                .unwrap();
            let image = image.as_rgba8().unwrap();
            (
                idx,
                Frame::from_parts(
                    image.clone(),
                    0,
                    0,
                    Delay::from_saturating_duration(FRAME_DELAY),
                ),
            )
        })
        .collect();
    frames.sort_unstable_by_key(|x| x.0);

    encoder
        .encode_frames(frames.into_iter().map(|x| x.1))
        .unwrap();
}
