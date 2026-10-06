# Debug the GD order workflow in Visual Studio and VS Code

This project generates a Visual Studio solution containing both C++ workflow
variants, using the same application sources as the
[order-workflow benchmark](../../docs/high-level/order-workflow.md):

- `gd_order_workflow`: GD `table_column_buffer` (AoS).
- `gd_order_workflow_simd`: GD `simd_table_8_8` (AoSoA), with the application's
  `simd_table.hpp` adapter.

## VS Code

On **macOS / Linux**, open **`external/order-workflow`** as its own folder in VS Code so that the
checked-in [launch configurations](.vscode/launch.json) and
[build tasks](.vscode/tasks.json) are loaded. From the repository root:

```sh
code external/order-workflow
```

Install CMake 3.24 or newer and Python 3 with SQLite support. Initialize
`external/gd` by running `git submodule update --init external/gd` from the
repository root. The C++ editor configuration lives inside this folder; the
top-level workspace remains dedicated to Rust.

- **Windows:** install Visual Studio 2022's **Desktop development with C++**
  workload and the [Microsoft C/C++ extension](https://code.visualstudio.com/docs/cpp/config-msvc).
  Open [order-workflow-windows.code-workspace](order-workflow-windows.code-workspace):
  `code external/order-workflow/order-workflow-windows.code-workspace` from the
  repository root.
  Choose `GD order-workflow: AoS (Windows)` or
  `GD order-workflow: AoSoA (Windows)` in **Run and Debug**.
- **macOS / Linux:** install a C++ compiler and Make, and the
  [CodeLLDB extension](https://github.com/vadimcn/codelldb/blob/master/MANUAL.md).
  On macOS, install the Xcode command-line tools with `xcode-select --install`.
  Choose `GD order-workflow: AoS (macOS / Linux)` or
  `GD order-workflow: AoSoA (macOS / Linux)` in **Run and Debug**.

The folder's `launch.json` uses LLDB. The Windows workspace contains the
Windows-only `cppvsdbg` configurations, keeping unsupported debugger types out
of the macOS / Linux folder configuration.

Open [workload.hpp](../../benches/cpp-reference/order_workflow/workload.hpp) and
set a breakpoint, then
press **F5**. Select the worker count and stage when prompted; defaults are four
workers and `verify`. The pre-launch task configures CMake, creates the eleven-line
SQL fixture, and builds the selected Debug executable. Build output is kept in
`target/order-workflow-vscode/debug`, separate from the Visual Studio solution
and the optimized benchmark builds.
Each launch depends on its build task, which depends on the configure task.
CMake tracks source, header, and library dependencies and rebuilds only what
changed; launching again without changes reuses the existing executable.

Use **Terminal → Run Task → GD workflow: verify both Debug variants** to build
both programs and run their four SQL verification cases. To debug a different
fixture, generate it with `../../benches/order_workflow/fixture.py` from this
folder and change the first
argument in the selected launch configuration. The VS Code configurations use
their own arguments; the `GD_WORKFLOW_*` CMake debugger defaults below apply to
the generated Visual Studio projects.

## Generate and open the solution

Install Visual Studio 2022 with **Desktop development with C++**, CMake 3.24 or
newer, and Python 3 with SQLite support. The first configure downloads the pinned
SQLite amalgamation. From the repository root in PowerShell:

```powershell
git submodule update --init external/gd
cd external/order-workflow
cmake --preset vs2022
cmake --build --preset debug
ctest --preset debug
Invoke-Item ../../target/order-workflow-vs/vs2022/gd_order_workflow_debug.sln
```

In Visual Studio, select **Debug / x64**. Right-click either workflow project and
choose **Set as Startup Project**, then press **F5**. Debug builds retain symbols,
disable optimization, and keep assertions enabled. Both projects already have
their debugger arguments and working directory configured through CMake's
[debugger arguments](https://cmake.org/cmake/help/latest/prop_tgt/VS_DEBUGGER_COMMAND_ARGUMENTS.html)
and [working directory](https://cmake.org/cmake/help/latest/prop_tgt/VS_DEBUGGER_WORKING_DIRECTORY.html)
properties.

The default command is `DATABASE 4 verify 1 native`. Configure creates the shared
eleven-line hand fixture inside the build directory, then `verify` exercises
import, preparation, and all eight variants and compares their cells against SQL.
The four CTest cases verify both programs with one and four workers.

Useful breakpoints are `workflow::Load`, `workflow::Prepare`,
`workflow::MakeHarvestTarget`, `workflow::Variant`, and `workflow::BatchPool::Run`.
For AoS, step into `gd::table::table_column_buffer::harvest`; for AoSoA, step into
`workflow::SimdTable::harvest` and the generated `gd_table_simd.cpp` source.

## Choose data, workers, or a stage

Generate a larger fixture at a new path, then reconfigure the debugger defaults:

```powershell
python ../../benches/order_workflow/fixture.py ../../target/order-workflow-vs/lines-10000.sqlite --rows 10000
cmake --preset vs2022 -DGD_WORKFLOW_DATABASE="$((Resolve-Path ../../target/order-workflow-vs/lines-10000.sqlite).Path)" -DGD_WORKFLOW_WORKERS=8 -DGD_WORKFLOW_STAGE=complete
```

Reopen the generated solution after reconfiguring. Alternatively edit each
project's **Properties → Debugging → Command Arguments**. The argument order is
`DATABASE WORKERS verify|import|prepare|variants|complete SAMPLES native|sorted`;
the C++ implementation uses sorted indexes in either index mode. Timed stages run
one warmup plus the requested number of samples. Debug timings are for debugging,
not for comparison with the optimized performance report.

All generated projects, SQLite files, compatibility headers, and the corrected
AoSoA header stay under `target`. The project reads `external/gd` without modifying
it. The generated header removes the same literal placeholder as the benchmark's
build recipe. To debug on another platform, configure this folder with that
platform's generator and `-DCMAKE_BUILD_TYPE=Debug`.

## Source formatting

All maintained C++ sources use the repository's
[clang-format configuration](../../.clang-format):
four-space indentation, a 100-column limit, braces on control flow, and expanded
functions, lambdas, and statements. From the repository root, format them with:

```sh
git ls-files '*.cpp' '*.hpp' '*.h' '*.cc' '*.cxx' '*.c' | xargs clang-format -i
```
