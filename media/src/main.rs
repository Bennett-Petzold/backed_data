use std::{
    fs::{create_dir_all, File},
    path::Path,
    process::Command,
};

use processing::{render, InvisSequence};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use renders::backed_load;

mod processing;
mod renders;

#[derive(Debug)]
pub struct ImageInput<'a> {
    pub base: &'a str,
    pub invis_sequences: &'a [InvisSequence<'a>],
    pub base_invis_seq: &'a [usize],
    pub comment_seq: &'a [&'a [usize]],
    pub target: &'a str,
}

fn main() {
    let output_dir = Path::new("../media_output");
    create_dir_all(output_dir).unwrap();

    let all_inputs = [backed_load::INPUT];

    all_inputs.par_iter().for_each(|input| {
        let dest = output_dir.join(input.target.to_string() + ".gif");

        render(
            &input.base.to_string().lines().collect::<Vec<&str>>(),
            input.invis_sequences,
            input.base_invis_seq,
            input.comment_seq,
            File::create(&dest).unwrap(),
        );

        Command::new("gifsicle")
            .args([
                "-b",
                dest.as_os_str().to_str().unwrap(),
                "-O3",
                "--colors 16",
            ])
            .output()
            .expect("Failed to run `gifsicle`, is it installed?");
    })
}
