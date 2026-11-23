//! Бенчмарки ядра: открытие большого файла и правки в нём.
//! Запуск: `cargo bench -p anvil-core`.

use std::hint::black_box;

use anvil_core::{Buffer, Editor};
use criterion::{Criterion, criterion_group, criterion_main};

fn big_text(mb: usize) -> String {
    let line = "    let value = compute(alpha, beta, gamma) + offset * 42; // комментарий\n";
    line.repeat(mb * 1024 * 1024 / line.len())
}

fn open_100mb(c: &mut Criterion) {
    let text = big_text(100);
    let mut g = c.benchmark_group("open");
    g.sample_size(10);
    g.bench_function("100 МБ из памяти", |b| b.iter(|| Buffer::from_text(black_box(&text))));
    g.finish();
}

fn edits(c: &mut Criterion) {
    let text = big_text(100);
    let mut e = Editor::from_text(&text);
    let n = e.rope().len_chars();
    let mut i = 0usize;
    c.bench_function("вставка символа в файл 100 МБ", |b| {
        b.iter(|| {
            i = (i * 7919 + 104_729) % n;
            e.set_cursor(i);
            e.insert_text("x");
        })
    });
    let mut small = Editor::from_text(&big_text(1));
    c.bench_function("Ctrl+D по файлу 1 МБ", |b| {
        b.iter(|| {
            small.set_cursor(10);
            small.select_next_occurrence();
            small.select_next_occurrence();
        })
    });
}

criterion_group!(benches, open_100mb, edits);
criterion_main!(benches);
