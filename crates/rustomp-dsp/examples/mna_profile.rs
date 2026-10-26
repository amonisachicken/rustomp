// SPDX-License-Identifier: GPL-3.0-or-later

//! 实时 MNA 路径的分项耗时（`cargo run -p rustomp-dsp --release --example mna_profile`）。
//!
//! 用来判断"慢在哪"：二极管解析模型 / LU 分解 / 整拍。

use rustomp_dsp::{Mna, Netlist, D_1N4148, GND, OP07};
use std::hint::black_box;

fn bench(name: &str, n: usize, mut f: impl FnMut()) {
    for _ in 0..n / 10 {
        f();
    }
    let t = std::time::Instant::now();
    for _ in 0..n {
        f();
    }
    println!(
        "  {name:34} {:6.0} ns",
        t.elapsed().as_nanos() as f64 / n as f64
    );
}

fn make() -> Netlist<12> {
    let mut net = Netlist::<12>::new();
    net.resistor(1, 2, 1000.0)
        .resistor(2, GND, 2200.0)
        .capacitor(2, 3, 1e-9)
        .capacitor(3, 5, 1e-6)
        .diode(D_1N4148, 3, GND)
        .diode(D_1N4148, 3, 2)
        .diode(D_1N4148, 2, 3)
        .opamp(OP07.model(1.0 / 192000.0), 1, 2, 3, 4)
        .voltage_source(5, 1, GND, 1.0)
        .voltage_source(6, 3, GND, 0.0);
    net.solve_dc();
    net
}

fn main() {
    let n = 200_000;
    let dt = 1.0 / 192_000.0;
    let mut net = make();

    bench("diode.current_and_conductance", n, || {
        black_box(D_1N4148.current_and_conductance(black_box(0.62)));
    });

    // 单次 LU
    let mut base = Mna::<12>::new();
    for i in 1..12 {
        base.conductance(i, GND, 1e-3 + i as f64 * 1e-4);
    }
    for i in 1..12 {
        base.conductance(i, i % 11 + 1, 5e-4);
    }
    for i in 0..12 {
        base.inject(i, 0.001 * i as f64);
    }
    bench("Mna::solve (12x12, 稠密)", n, || {
        black_box(base.solve());
    });
    bench("Mna::clone", n, || {
        black_box(base.clone());
    });

    bench("step_fixed 1 迭代", n, || {
        black_box(net.step_fixed(dt, 1));
    });
    bench("step_fixed 2 迭代", n, || {
        black_box(net.step_fixed(dt, 2));
    });
    bench("step_fixed 3 迭代", n, || {
        black_box(net.step_fixed(dt, 3));
    });
}
