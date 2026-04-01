# Sui Prover (unconfirmedlabs fork)

Formal verification for Sui Move smart contracts. Based on the Boogie verification engine and the Z3 SMT solver.

This is a fork of [asymptotic-code/sui-prover](https://github.com/asymptotic-code/sui-prover) with the following changes:

### What's different from upstream

**MVR support** — Uses `move-package-alt` for dependency resolution, which supports the current MVR protocol (`r.mvr = "@namespace/package"`). The upstream prover uses the legacy `move-package` crate which calls MVR with a deprecated interface.

**Modernized prover packages** — The `prover`, `specs`, and `sui_prover` Move packages were recreated with `sui move new` to align with the current Sui package management system. No deprecated `[addresses]` sections.

**ProverFlavor** — A lightweight `MoveFlavor` implementation that provides Sui system deps and prover framework injection without depending on the full Sui SDK.

## Install

### Build from source

```bash
git clone https://github.com/unconfirmedlabs/sui-prover.git
cd sui-prover
cargo install --path crates/sui-prover
```

### Dependencies

The prover requires Boogie and Z3. Install via Homebrew:

```bash
brew install z3 dotnet@8
```

Boogie must be built from source (it's a .NET application):

```bash
git clone https://github.com/boogie-org/boogie.git
cd boogie
dotnet build Source/Boogie.sln -c Release
# The binary is at Source/BoogieDriver/bin/Release/net8.0/BoogieDriver
```

Set the required environment variables:

```bash
export DOTNET_ROOT=$(brew --prefix dotnet@8)/libexec
export BOOGIE_EXE=/path/to/BoogieDriver
export Z3_EXE=$(which z3)
```

## Usage

Run from a directory containing `Move.toml`:

```bash
sui-prover           # compact output
sui-prover -v        # verbose — shows each check
sui-prover -t 300    # custom timeout (seconds)
sui-prover -g        # generate Boogie code only (no verification)
```

### Writing specifications

Specs live alongside source files in `sources/`. Each spec is a function annotated with `#[spec(prove)]` that calls the function under test and asserts properties with `requires()` and `ensures()`:

```move
module my_package::my_spec;

#[spec_only]
use prover::prover::{requires, ensures};
use my_package::my_module;

#[spec(prove)]
fun add_spec(a: u64, b: u64): u64 {
    requires(a <= 1000);
    requires(b <= 1000);
    let result = my_module::add(a, b);
    ensures(result == a + b);
    result
}
```

#### Proving a function aborts

Use `ignore_abort` with `ensures(false)` — if the function doesn't abort, the ensures would fail, proving it must abort:

```move
#[spec(prove, ignore_abort)]
fun divide_by_zero_aborts_spec() {
    let _result = my_module::divide(10, 0);
    ensures(false); // unreachable if function aborts
}
```

#### Capturing pre-state

Use `clone!()` to snapshot values before mutation:

```move
#[spec(prove, ignore_abort)]
fun push_increments_length_spec(v: &mut vector<u64>, val: u64) {
    let old_len = clone!(v).length();
    v.push_back(val);
    ensures(v.length() == old_len + 1);
}
```

### MVR dependencies

This fork supports MVR out of the box. Use `r.mvr` in your `Move.toml`:

```toml
[dependencies]
my_dep = { r.mvr = "@namespace/package" }
```

No special configuration needed — the prover calls the `mvr` CLI for resolution, just like `sui move build`.

## Output

Each spec generates 3 verification checks:

- **`_Check`** — Does the `ensures()` hold for all valid inputs? Z3 exhaustively searches for a counterexample.
- **`_Assume`** — Are the `requires()` conditions satisfiable? Catches vacuously true specs.
- **`_SpecNoAbortCheck`** — Does the spec function itself not abort?

A passing run means the property is **mathematically proven** for all inputs satisfying the preconditions.

## Upstream documentation

For the full spec language reference (quantifiers, ghost variables, loop invariants, datatype invariants), see the [Sui Prover Documentation](https://info.asymptotic.tech/sui-prover).

## License

Apache-2.0
