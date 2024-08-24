use crate::{processing::InvisSequence, ImageInput};

pub const LINES: &str = r#"digraph G {
	rankdir = "LR";
	compound = true;
    newrank = true;
    ranksep = 0.02;
    fontname = "Helvetica 24 bold";
    bgcolor = grey;

	node [shape = rect, style = rounded; fontname = "Helvetica 24 bold"];
	edge [fontname = "Helvetica 24 bold"];

	Program [group = g1];

	subgraph cluster_memory {
		style = "filled, dashed";
		fillcolor = tomato;
		label = "Program Memory";
		margin = 24;

        subgraph cluster_arr {
            color = black;
            style = "filled,dashed";
            fillcolor = tomato;
            label = "VecBackedArray\n<T, Plainfile, BincodeCoder>";

            Keys [ label = "Keys (Selector)", group = g1 ];

            subgraph backings {
                rank = same;
                subgraph cluster_backed_0 {
                    color = black;
                    style = "filled,dashed";
                    fillcolor = tomato;
                    label = "BackedEntry (0)\n<OnceCell<Vec<T>>, Plainfile, BincodeCoder>";

                    Box0 [ label = "None", group = "backed" ];
                    Box0 [ label = "Vec<T>", group = "backed" ];
                }

                subgraph cluster_backed_2 {
                    color = black;
                    style = "filled,dashed";
                    fillcolor = tomato;
                    label = "BackedEntry (2)\n<OnceCell<Vec<T>>, Plainfile, BincodeCoder>";

                    Box2 [ label = "None", group = "backed" ];
                    Box2 [ label = "Vec<T>", group = "backed" ];
                }

                subgraph cluster_backed_1 {
                    color = black;
                    style = "filled,dashed";
                    fillcolor = tomato;
                    label = "BackedEntry (1)\n<OnceCell<Vec<T>>, Plainfile, BincodeCoder>";

                    Box1 [ label = "None", group = "backed" ];
                }
            }

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
		File0 [ label = "Bincode File (0)" ];
		File1 [ label = "Bincode File (1)" ];
		File2 [ label = "Bincode File (2)" ];
	}

	Program -> Keys [ label = ".get(n in 0)", labeldistance = 4.0 ];
	Program -> Keys [ label = ".get(n in 2)", labeldistance = 4.0 ];
	Program -> Keys [ label = ".clear()       " ];

	Keys:ne -> Box0 [ taillabel = ".load()", labeldistance = 16, labelangle = -5.0 ];
	Keys:ne -> Box0 [ taillabel = ".unload()", labeldistance = 12, labelangle = 2.0 ];
	Plainfile -> File0 [ label = "Read to Buffer ([u8])", dir=back ];
	Box0 -> BincodeCoder [ label = "Store ([T])", dir=back ];

	Keys -> Box1 [ xlabel = ".unload()" ];
	Plainfile -> File1 [ style = invis, dir=back ];
	Box1 -> BincodeCoder [ style = invis, dir=back ];

	Keys:se -> Box2:w [ xlabel = ".load()" ];
	Keys:se -> Box2:w [ xlabel = ".unload()" ];
	Plainfile -> File2 [ label = "Read to Buffer ([u8])", dir=back ];
	Box2 -> BincodeCoder [ label = "Store ([T])", dir=back ];

	BincodeCoder -> Plainfile [ label = "Decode ([u8])", dir=back ];

	Program:ne -> Box0 [ label = "Return (&T[n])", dir=back ];
	Program:s -> Box2 [ label = "Return (&T[n])", dir=back ];
	Program -> Box1 [ style = invis, dir=back ];
}"#;

pub const BASE_SEQ: &[usize] = &[78, 79, 80, 82, 83, 84, 87, 90, 91, 92, 94, 96, 97];

pub const INVIS_SEQUENCES: &[InvisSequence] = &[
    // Zero load
    InvisSequence::Base(&[63, 64, 78, 82, 84, 85, 87, 91, 93, 94, 96, 98, 99]),
    InvisSequence::NonBase(&[63, 64, 82, 84, 85, 87, 91, 93, 94, 96, 98, 99]),
    InvisSequence::NonBase(&[63, 64, 84, 85, 87, 91, 93, 94, 96, 98, 99]),
    InvisSequence::NonBase(&[84, 85, 87, 91, 93, 94, 96, 98, 99]),
    InvisSequence::NonBase(&[85, 87, 91, 93, 94, 96, 98, 99]),
    InvisSequence::NonBase(&[85, 87, 91, 93, 94, 98, 99]),
    InvisSequence::NonBase(&[87, 91, 93, 94, 98, 99]),
    InvisSequence::NonBase(&[87, 91, 93, 94, 98, 99]),
    InvisSequence::NonBase(&[87, 91, 93, 94, 99]),
    // Two load
    InvisSequence::Base(&[63, 64, 79, 82, 84, 85, 87, 91, 93, 94, 96, 98, 99]),
    InvisSequence::NonBase(&[63, 64, 82, 84, 85, 87, 91, 93, 94, 96, 98, 99]),
    InvisSequence::NonBase(&[63, 64, 82, 84, 85, 87, 93, 94, 96, 98, 99]),
    InvisSequence::NonBase(&[82, 84, 85, 87, 93, 94, 96, 98, 99]),
    InvisSequence::NonBase(&[82, 84, 85, 87, 94, 96, 98, 99]),
    InvisSequence::NonBase(&[82, 84, 85, 87, 94, 98, 99]),
    InvisSequence::NonBase(&[82, 84, 85, 87, 98, 99]),
    InvisSequence::NonBase(&[82, 84, 85, 87, 98, 99]),
    InvisSequence::NonBase(&[82, 84, 85, 87, 98]),
    // Zero load again
    InvisSequence::Base(&[63, 64, 78, 82, 84, 85, 87, 91, 93, 94, 96, 98, 99]),
    InvisSequence::NonBase(&[63, 64, 82, 84, 85, 87, 91, 93, 94, 96, 98, 99]),
    InvisSequence::NonBase(&[63, 64, 84, 85, 87, 91, 93, 94, 96, 98, 99]),
    InvisSequence::NonBase(&[63, 64, 84, 85, 87, 91, 93, 94, 96, 99]),
    // Clear
    InvisSequence::Base(&[63, 64, 80, 83, 84, 85, 87, 92, 93, 94, 96, 98, 99]),
    InvisSequence::NonBase(&[63, 64, 83, 84, 85, 87, 92, 93, 94, 96, 98, 99]),
    InvisSequence::NonBase(&[63, 64, 84, 85, 93, 94, 96, 98, 99]),
    InvisSequence::NonBase(&[63, 64, 84, 85, 93, 94, 96, 98, 99]),
];

pub const COMMENT_SEQ: &[&[usize]] = &[
    // Zero load
    &[36, 46, 79, 80, 83, 92],
    &[36, 46, 79, 80, 83, 92],
    &[36, 46, 79, 80, 83, 92],
    &[36, 46, 79, 80, 83, 92],
    &[36, 46, 79, 80, 83, 92],
    &[36, 46, 79, 80, 83, 92],
    &[36, 46, 79, 80, 83, 92],
    &[35, 46, 79, 80, 83, 92],
    &[35, 46, 79, 80, 83, 92],
    // One load
    &[35, 46, 78, 80, 83, 92],
    &[35, 46, 78, 80, 83, 92],
    &[35, 46, 78, 80, 83, 92],
    &[35, 46, 78, 80, 83, 92],
    &[35, 46, 78, 80, 83, 92],
    &[35, 46, 78, 80, 83, 92],
    &[35, 46, 78, 80, 83, 92],
    &[35, 45, 78, 80, 83, 92],
    &[35, 45, 78, 80, 83, 92],
    // Zero load again
    &[35, 45, 79, 80, 83, 92],
    &[35, 45, 79, 80, 83, 92],
    &[35, 45, 79, 80, 83, 92],
    &[35, 45, 79, 80, 83, 92],
    // Clear
    &[35, 45, 78, 79, 82, 91],
    &[35, 45, 78, 79, 82, 91],
    &[35, 45, 78, 79, 82, 91],
    &[36, 46, 78, 79, 82, 91],
];

pub const INPUT: ImageInput = ImageInput {
    base: LINES,
    invis_sequences: INVIS_SEQUENCES,
    base_invis_seq: BASE_SEQ,
    comment_seq: COMMENT_SEQ,
    target: "array_load",
};
