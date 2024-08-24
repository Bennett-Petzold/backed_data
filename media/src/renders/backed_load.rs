use crate::{processing::InvisSequence, ImageInput};

pub const LINES: &str = r#"digraph G {
	rankdir = "LR";
	compound = true;
    fontname = "Helvetica 24 bold";
    bgcolor = grey;

	node [shape = rect, style = rounded; fontname = "Helvetica 24 bold"];
	edge [fontname = "Helvetica 24 bold"];

	Program;

	subgraph cluster_memory {
		style = "filled, dashed";
		fillcolor = tomato;
		label = "Program Memory";
		margin = 24;

		subgraph cluster_backed {
			color = black;
			style = "filled,dashed";
            fillcolor = tomato;
			label = "BackedEntry\n<OnceCell<Box<T>>, Plainfile, BincodeCoder>";

			subgraph cluster_once {
				label = "OnceCell<Box<T>>";
				Box [ label = "None" ];
				Box [ label = "Box<T>" ];
			};

			subgraph cluster_temp_memory {
				style = "filled,dashed";
				fillcolor = lightgreen;
				label = "Transient Memory";
				Plainfile;
				BincodeCoder;
			}
		}
	}

	subgraph cluster_disk {
		style = "filled,dashed";
		fillcolor = lightblue;
		label = "Disk";
		File [ label = "Bincode File" ];
	}

	Program -> Box [ label = ".load()", labeldistance = 4.0 ];
	Program -> Box [ label = ".unload()" ];
	Plainfile -> File [ label = "Read to Buffer ([u8])", dir=back ];
	BincodeCoder -> Plainfile [ label = "Decode ([u8])", dir=back ];
	Box -> BincodeCoder [ label = "Store (T)", dir=back ];
	Program -> Box [ label = "Return (&T)", dir=back ];
}"#;

pub const BASE_SEQ: &[usize] = &[44, 45, 46, 47, 48, 49];

pub const INVIS_SEQUENCES: &[InvisSequence] = &[
    InvisSequence::Base(&[33, 34, 46, 47, 48, 49, 50, 51]),
    InvisSequence::NonBase(&[33, 34, 47, 48, 49, 50, 51]),
    InvisSequence::NonBase(&[47, 48, 49, 50, 51]),
    InvisSequence::NonBase(&[47, 49, 50, 51]),
    InvisSequence::NonBase(&[47, 50, 51]),
    InvisSequence::NonBase(&[47, 51]),
    InvisSequence::NonBase(&[47, 51]),
    InvisSequence::NonBase(&[]),
    InvisSequence::Base(&[33, 34, 46, 47, 48, 49, 50, 51]),
    InvisSequence::NonBase(&[33, 34, 47, 48, 49, 50, 51]),
    InvisSequence::NonBase(&[33, 34, 47, 48, 49, 50]),
    InvisSequence::Base(&[33, 34, 46, 47, 48, 49, 50, 51]),
    InvisSequence::NonBase(&[33, 34, 48, 49, 50, 51]),
    InvisSequence::NonBase(&[33, 34, 48, 49, 50, 51]),
];

pub const COMMENT_SEQ: &[&[usize]] = &[
    &[26, 47],
    &[26, 47],
    &[26, 47],
    &[26, 47],
    &[26, 47],
    &[26, 47],
    &[25, 47],
    &[25, 47],
    &[25, 47],
    &[25, 47],
    &[25, 47],
    &[25, 47],
    &[25, 46],
    &[26, 46],
];

pub const INPUT: ImageInput = ImageInput {
    base: LINES,
    invis_sequences: INVIS_SEQUENCES,
    base_invis_seq: BASE_SEQ,
    comment_seq: COMMENT_SEQ,
    target: "backed_load",
};
