/*
    Copyright 2023 - Raphaël Rigo

    Licensed under the Apache License, Version 2.0 (the "License");
    you may not use this file except in compliance with the License.
    You may obtain a copy of the License at

        http://www.apache.org/licenses/LICENSE-2.0

    Unless required by applicable law or agreed to in writing, software
    distributed under the License is distributed on an "AS IS" BASIS,
    WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
    See the License for the specific language governing permissions and
    limitations under the License.
*/
// Includes (many) changes by Valentin Obst.
use crate::Arch;

use std::collections::HashMap;
use std::fmt::Debug;
use std::time::Instant;

use log::{debug, info};
use rayon::prelude::*;
use rust_embed::Embed;

#[derive(Embed)]
#[folder = "$OUT_DIR/cpu_rec_corpus"]
struct Corpus;

#[allow(dead_code)]
#[derive(Debug)]
pub struct CorpusStats {
    pub arch: String,
    pub ungrams_freq: HashMap<u8, f64>,
    pub bigrams_freq: HashMap<(u8, u8), f64>,
    pub trigrams_freq: HashMap<(u8, u8, u8), f64>,
    pub ug_base_freq: f64,
    pub bg_base_freq: f64,
    pub tg_base_freq: f64,
}

/// For some arches we need to be a bit more strict as they cause many false
/// positives.
pub fn is_strict(arch: &Arch) -> bool {
    const STRICT_ARCHES: &[&str] = &["OCaml", "IA-64", "IQ2000"];

    STRICT_ARCHES.contains(&arch.as_str())
}

pub fn load_corpus() -> Vec<CorpusStats> {
    let now = Instant::now();

    let corpus_entries: Vec<_> = Corpus::iter()
        .map(|arch| {
            let arch = match arch {
                std::borrow::Cow::Borrowed(arch) => arch,
                _ => core::unreachable!(),
            };
            match Corpus::get(arch).unwrap().data {
                std::borrow::Cow::Borrowed(data) => (arch.trim_end_matches(".corpus"), data),
                _ => core::unreachable!(),
            }
        })
        .collect();

    let corpus_stats: Vec<CorpusStats> = corpus_entries
        .into_par_iter()
        .map(|(arch, data)| {
            debug!("Loading corpus entry for arch {}.", arch);

            // Corpus statistics are computed with a base count of 0.01 as
            // it will be used as divisor during guessing.
            CorpusStats::new(arch.to_owned(), data, 0.01)
        })
        .collect();

    info!("Loaded corpus in {}s.", now.elapsed().as_secs());

    corpus_stats
}

pub struct Divergences {
    pub bigrams: f64,
    pub trigrams: f64,
}

impl CorpusStats {
    pub fn new(arch: String, data: &[u8], base_count: f64) -> Self {
        let mut ug_counts = HashMap::new();
        let mut bg_counts = HashMap::new();
        let mut tg_counts = HashMap::new();

        for w in data.windows(3) {
            let ug = w[0];
            ug_counts
                .entry(ug)
                .and_modify(|count| *count += 1.0)
                .or_insert(1.0 + base_count);

            let bg = (w[0], w[1]);
            bg_counts
                .entry(bg)
                .and_modify(|count| *count += 1.0)
                .or_insert(1.0 + base_count);

            let tg = (w[0], w[1], w[2]);
            tg_counts
                .entry(tg)
                .and_modify(|count| *count += 1.0)
                .or_insert(1.0 + base_count);
        }

        debug!(
            "{}: {} bytes, {:x} ungrams, {:x} bigrams, {:x} trigrams",
            arch,
            data.len(),
            ug_counts.len(),
            bg_counts.len(),
            tg_counts.len()
        );

        let ug_qtotal: f64 = (base_count * ((u32::pow(256, 1) - ug_counts.len() as u32) as f64))
            + ug_counts.values().sum::<f64>();
        debug!("{} ungrams Qtotal: {}", arch, ug_qtotal);

        let bi_qtotal: f64 = (base_count * ((u32::pow(256, 2) - bg_counts.len() as u32) as f64))
            + bg_counts.values().sum::<f64>();
        debug!("{} bigrams Qtotal: {}", arch, bi_qtotal);

        let tri_qtotal: f64 = (base_count * ((u32::pow(256, 3) - tg_counts.len() as u32) as f64))
            + tg_counts.values().sum::<f64>();
        debug!("{} trigrams Qtotal: {}", arch, tri_qtotal);

        // Update counts to frequencies.
        let ug_freq = ug_counts
            .into_iter()
            .map(|(k, v)| (k, (v / ug_qtotal)))
            .collect();
        let bg_freq = bg_counts
            .into_iter()
            .map(|(k, v)| (k, (v / bi_qtotal)))
            .collect();
        let tg_freq = tg_counts
            .into_iter()
            .map(|(k, v)| (k, (v / tri_qtotal)))
            .collect();

        CorpusStats {
            arch,
            ungrams_freq: ug_freq,
            bigrams_freq: bg_freq,
            trigrams_freq: tg_freq,
            ug_base_freq: base_count / ug_qtotal,
            bg_base_freq: base_count / bi_qtotal,
            tg_base_freq: base_count / tri_qtotal,
        }
    }

    /// Compute the Kullback–Leibler divergence (cross entropy) of the
    /// current file with the reference from corpus `q`.
    pub fn compute_kl(&self, q: &Self) -> Divergences {
        let mut kld_bg = 0.0;
        for (bg, f) in &self.bigrams_freq {
            if *f != 0.0 {
                kld_bg += f * (f / q.bigrams_freq.get(bg).unwrap_or(&q.bg_base_freq)).ln();
            }
        }
        let mut kld_tg = 0.0;
        for (tg, f) in &self.trigrams_freq {
            if *f != 0.0 {
                kld_tg += f * (f / q.trigrams_freq.get(tg).unwrap_or(&q.tg_base_freq)).ln();
            }
        }
        Divergences {
            bigrams: kld_bg,
            trigrams: kld_tg,
        }
    }
}
