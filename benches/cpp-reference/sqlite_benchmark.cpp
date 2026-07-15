#include <benchmark/benchmark.h>

#include <cstdint>
#include <stdexcept>
#include <string>
#include <vector>

#include <sqlite3.h>

namespace {

void CheckSqlite(int iResult, sqlite3* psqliteDatabase)
{
   if(iResult != SQLITE_OK)
   {
      throw std::runtime_error(psqliteDatabase == nullptr
                                  ? sqlite3_errstr(iResult)
                                  : sqlite3_errmsg(psqliteDatabase));
   }
}

class SqliteFixture
{
public:
   explicit SqliteFixture(std::size_t uRowCount)
   {
      CheckSqlite(sqlite3_open(":memory:", &m_psqliteDatabase), m_psqliteDatabase);
      CheckSqlite(sqlite3_exec(m_psqliteDatabase,
                               "CREATE TABLE item(id INTEGER NOT NULL, "
                               "group_name TEXT NOT NULL, value INTEGER NOT NULL);"
                               "BEGIN",
                               nullptr,
                               nullptr,
                               nullptr),
                  m_psqliteDatabase);

      sqlite3_stmt* pstatementInsert = nullptr;
      CheckSqlite(sqlite3_prepare_v2(m_psqliteDatabase,
                                     "INSERT INTO item VALUES (?1, ?2, ?3)",
                                     -1,
                                     &pstatementInsert,
                                     nullptr),
                  m_psqliteDatabase);
      for(std::size_t uRow = 0; uRow < uRowCount; uRow++)
      {
         const std::int64_t iIdentifier = static_cast<std::int64_t>(uRow);
         const std::string stringGroup = "group-" + std::to_string(uRow % 16);
         CheckSqlite(sqlite3_bind_int64(pstatementInsert, 1, iIdentifier), m_psqliteDatabase);
         CheckSqlite(sqlite3_bind_text(pstatementInsert,
                                       2,
                                       stringGroup.data(),
                                       static_cast<int>(stringGroup.size()),
                                       SQLITE_TRANSIENT),
                     m_psqliteDatabase);
         CheckSqlite(sqlite3_bind_int64(pstatementInsert, 3, -iIdentifier), m_psqliteDatabase);
         if(sqlite3_step(pstatementInsert) != SQLITE_DONE)
         {
            throw std::runtime_error(sqlite3_errmsg(m_psqliteDatabase));
         }
         CheckSqlite(sqlite3_reset(pstatementInsert), m_psqliteDatabase);
         CheckSqlite(sqlite3_clear_bindings(pstatementInsert), m_psqliteDatabase);
      }
      CheckSqlite(sqlite3_finalize(pstatementInsert), m_psqliteDatabase);
      CheckSqlite(sqlite3_exec(m_psqliteDatabase, "COMMIT", nullptr, nullptr, nullptr),
                  m_psqliteDatabase);
   }

   SqliteFixture(const SqliteFixture&) = delete;
   SqliteFixture& operator=(const SqliteFixture&) = delete;

   ~SqliteFixture()
   {
      if(m_psqliteDatabase != nullptr) sqlite3_close(m_psqliteDatabase);
   }

   sqlite3* get() const noexcept { return m_psqliteDatabase; }

private:
   sqlite3* m_psqliteDatabase = nullptr;
};

struct TypedTable
{
   std::vector<std::int64_t> m_vectorIdentifier;
   std::vector<std::string> m_vectorGroup;
   std::vector<std::int64_t> m_vectorValue;
};

TypedTable QueryTable(sqlite3* psqliteDatabase, std::size_t uExpectedRows)
{
   sqlite3_stmt* pstatementQuery = nullptr;
   CheckSqlite(sqlite3_prepare_v2(psqliteDatabase,
                                  "SELECT id, group_name, value FROM item",
                                  -1,
                                  &pstatementQuery,
                                  nullptr),
               psqliteDatabase);

   TypedTable tableResult;
   tableResult.m_vectorIdentifier.reserve(uExpectedRows);
   tableResult.m_vectorGroup.reserve(uExpectedRows);
   tableResult.m_vectorValue.reserve(uExpectedRows);
   int iStep = SQLITE_ROW;
   while((iStep = sqlite3_step(pstatementQuery)) == SQLITE_ROW)
   {
      if(sqlite3_column_type(pstatementQuery, 0) != SQLITE_INTEGER ||
         sqlite3_column_type(pstatementQuery, 1) != SQLITE_TEXT ||
         sqlite3_column_type(pstatementQuery, 2) != SQLITE_INTEGER)
      {
         sqlite3_finalize(pstatementQuery);
         throw std::runtime_error("unexpected SQLite storage class");
      }
      tableResult.m_vectorIdentifier.push_back(sqlite3_column_int64(pstatementQuery, 0));
      const char* pbszGroup = reinterpret_cast<const char*>(sqlite3_column_text(pstatementQuery, 1));
      const int iGroupLength = sqlite3_column_bytes(pstatementQuery, 1);
      tableResult.m_vectorGroup.emplace_back(pbszGroup, static_cast<std::size_t>(iGroupLength));
      tableResult.m_vectorValue.push_back(sqlite3_column_int64(pstatementQuery, 2));
   }
   if(iStep != SQLITE_DONE)
   {
      sqlite3_finalize(pstatementQuery);
      throw std::runtime_error(sqlite3_errmsg(psqliteDatabase));
   }
   CheckSqlite(sqlite3_finalize(pstatementQuery), psqliteDatabase);
   return tableResult;
}

void QueryTableSchema(benchmark::State& state)
{
   const std::size_t uRowCount = static_cast<std::size_t>(state.range(0));
   const SqliteFixture fixtureDatabase(uRowCount);
   for(auto _ : state)
   {
      auto tableResult = QueryTable(fixtureDatabase.get(), uRowCount);
      benchmark::DoNotOptimize(tableResult);
   }
   state.SetItemsProcessed(state.iterations() * state.range(0));
}

BENCHMARK(QueryTableSchema)
   ->Name("SQLite/QueryTable/schema")
   ->Arg(100)
   ->Arg(1000)
   ->Arg(10000);

} // namespace
