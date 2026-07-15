#include <benchmark/benchmark.h>

#include <cstdint>
#include <string>
#include <string_view>
#include <vector>

#include "expression/gd_expression_method_01.h"
#include "expression/gd_expression_runtime.h"
#include "expression/gd_expression_token.h"

namespace {

using gd::expression::runtime;
using gd::expression::tag_formula;
using gd::expression::tag_postfix;
using gd::expression::token;
using gd::expression::value;

runtime MakeRuntime()
{
   runtime result;
   result.add({
      static_cast<unsigned>(gd::expression::uMethodDefaultSize_g),
      gd::expression::pmethodDefault_g,
      std::string{},
   });
   result.add({
      static_cast<unsigned>(gd::expression::uMethodStringSize_g),
      gd::expression::pmethodString_g,
      std::string{},
   });
   result.add("x", std::int64_t{10});
   result.add("y", std::int64_t{20});
   return result;
}

std::vector<token> CompileFormula(std::string_view formula)
{
   std::vector<token> infix;
   std::vector<token> postfix;
   const auto parsed = token::parse_s(formula, infix, tag_formula{});
   if(!parsed.first) throw std::runtime_error(parsed.second);
   const auto compiled = token::compile_s(infix, postfix, tag_postfix{});
   if(!compiled.first) throw std::runtime_error(compiled.second);
   return postfix;
}

void CompileFormula(benchmark::State& state, std::string_view formula)
{
   for(auto _ : state)
   {
      std::vector<token> infix;
      std::vector<token> postfix;
      const auto parsed = token::parse_s(formula, infix, tag_formula{});
      if(!parsed.first) state.SkipWithError(parsed.second);
      const auto compiled = token::compile_s(infix, postfix, tag_postfix{});
      benchmark::DoNotOptimize(postfix);
      if(!compiled.first) state.SkipWithError(compiled.second);
   }
}

void EvaluateFormula(benchmark::State& state, std::string_view formula)
{
   auto runtime = MakeRuntime();
   const auto postfix = CompileFormula(formula);
   for(auto _ : state)
   {
      value result;
      const auto evaluated = token::calculate_s(postfix, &result, runtime);
      benchmark::DoNotOptimize(result);
      if(!evaluated.first) state.SkipWithError(evaluated.second);
   }
}

#define GD_EXPRESSION_BENCHMARK(Name, Formula)                                      \
   void Compile##Name(benchmark::State& state) { CompileFormula(state, Formula); }  \
   BENCHMARK(Compile##Name);                                                        \
   void Evaluate##Name(benchmark::State& state) { EvaluateFormula(state, Formula); }\
   BENCHMARK(Evaluate##Name)

GD_EXPRESSION_BENCHMARK(Short, "x + y * 2");
GD_EXPRESSION_BENCHMARK(Function, "abs(x - y) + max(x, y)");
GD_EXPRESSION_BENCHMARK(Logical, "x > y && x < 100");

#undef GD_EXPRESSION_BENCHMARK

} // namespace
