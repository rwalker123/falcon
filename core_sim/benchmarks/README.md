# Benchmarks

## Turn Benchmark

Run with:

```bash
cargo bench -p core_sim --bench turn_bench
```

Generates HTML results under `target/criterion/turn/report`. Grid sizes are
8, 16, 32, 48, 64 to chart scaling behaviour.

## Power Benchmark

Run with:

```bash
cargo bench -p core_sim --bench power_bench
```

Results land under `target/criterion/power_stability`. Balanced and cascading
deficit scenarios exercise the power phase with varying grid sizes. Use the
reports to watch for regressions specific to cascade redistribution and
stability calculations.
