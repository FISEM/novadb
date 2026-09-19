# What the existing query languages get wrong

Written before nova's grammar is fixed, so the design answers real failures
rather than imagined ones. Each entry asks two questions: what did this
language get right that nova should keep, and what does it get wrong that
nova must not repeat.

---

## SQL

**Right.** Declarative — you say what, the engine decides how, with fifty
years of optimiser work behind that split. And universal: every BI tool,
ORM, notebook and language model is fluent in it. That is a moat nothing
else on this page has.

**Wrong:**

1. **Reading order is not execution order.** `SELECT … FROM … WHERE …
   GROUP BY … HAVING … ORDER BY` runs roughly `FROM → WHERE → GROUP BY →
   HAVING → SELECT → ORDER BY`. Three consequences, all bad: you cannot
   build a query incrementally, autocomplete cannot know your columns when
   you type `SELECT`, and an alias defined in `SELECT` is unusable in
   `WHERE` because `WHERE` already ran. Beginners learn the pipeline
   backwards and never quite recover.
2. **No composition.** The only unit of abstraction is the subquery or CTE.
   No functions, no named fragments, no reuse. In practice the reuse
   mechanism is copy-paste.
3. **Three-valued logic.** `NULL = NULL` is unknown. `x NOT IN (SELECT …)`
   returns nothing at all if the subquery yields a single `NULL` — a silent
   wrong answer, not an error. `COUNT(*)` and `COUNT(col)` disagree on
   `NULL` and nothing warns you.
4. **Nested data is bolted on.** `->`, `->>`, `jsonb_path_query`, `LATERAL`
   plus `jsonb_array_elements` to unnest. Different in every engine.
5. **Graph is effectively out of reach.** `WITH RECURSIVE` is correct and
   almost nobody writes it from memory.
6. **"Standard SQL" is fiction.** Portable SQL is a subset no one actually
   writes.
7. **Destructive asymmetry.** You rewrite a `SELECT` into a `DELETE`, and
   the `WHERE` — the part that matters — is the part you retype.

## SurrealQL

**Right.** One language over relational, document and graph, which is
novadb's exact ambition. Record links (`person:alice`) remove join
boilerplate for the common case. The `->knows->person` arrows read well.

**Wrong:**

1. **It inherits SQL's clause order.** Still `SELECT … FROM … WHERE`. So it
   imports SQL's worst property while giving up SQL's compatibility — it
   pays the cost of being new without spending it on the main defect.
2. **Enormous surface.** DDL, permissions, events, functions, embedded
   scripting, live queries, futures, all in one language. "Easy" is not
   reachable at that size, whatever the syntax looks like.
3. **Proprietary, with no ecosystem.** No BI tool, no ORM, no fluency in
   any model or any developer's head. You pay for a new language and
   receive none of SQL's leverage.

## Cypher / GQL

**Right.** ASCII-art patterns — `(a)-[:KNOWS]->(b)` — are the single best
idea in query-language design. They look like the thing they describe, which
is why people learn them in minutes. Standardised as ISO GQL in 2024.

**Wrong:**

1. **Implicit grouping.** `RETURN a.name, count(*)` infers the grouping key
   from whichever terms are not aggregates. Add a field to `RETURN` and the
   meaning of your aggregation changes silently. This is the same class of
   defect as SQL's `NULL`: the rule is invisible at the call site.
2. **`WITH` is the pipeline joint but does not look like one.** It is a
   projection that happens to be the only way to chain stages.
3. **Graph only.** No credible tabular or document story.

## MongoDB — find and the aggregation pipeline

**Right.** The aggregation pipeline is the model nova chose: `$match`,
`$group`, `$sort`, `$project` compose, and reading order is execution order.
It works, at scale, for a very large number of developers.

**Wrong:**

1. **It is JSON, not a language.** Past two or three stages it stops being
   readable, and it cannot be typed comfortably by hand.
2. **Two sublanguages for one idea.** Query operators say
   `{age: {$gt: 30}}`; aggregation expressions say `{$gt: ["$age", 30]}`.
   Same concept, different syntax, different rules, and you must always know
   which context you are in. Comparing two fields in `find()` requires
   escaping into `$expr`.

## PRQL

**Right.** It proves the pipeline model as a SQL replacement — `from`,
`filter`, `derive`, `group`, `aggregate`, `sort`, `take` — and it has real
functions and variables, so it fixes SQL's composition gap outright. It
compiles to SQL, so it runs on everything.

**Wrong, for nova's purpose:** compiling to SQL means inheriting SQL's
model. No documents, no traversal, and errors can surface as the target
engine's SQL errors rather than the language's own. Still an early-adopter
project.

**The useful conclusion.** PRQL validates the syntax nova picked, and its
one structural limit is precisely the gap novadb can fill: novadb owns its
engine, so its pipeline can carry `walk` and document stages that a
SQL-targeting compiler cannot express at all.

## EdgeQL (Gel)

**Right.** Composable, and it returns nested results by default — which
kills the "join, flatten, then regroup in application code" ritual that
every SQL user performs daily. Real type system, links instead of foreign
keys.

**Wrong.** Large: a schema language plus a query language, both to learn.

**And the data point worth having on the table.** EdgeDB renamed itself to
Gel in February 2025, and Gel 6 shipped full SQL support over the Postgres
protocol. A company that bet its product on a custom query language added
SQL back — explicitly to let people adopt it gradually rather than commit
to EdgeQL up front. That is the cost of leaving SQL, priced by someone who
already paid it.

## Gremlin, Datalog, GraphQL — briefly

- **Gremlin** composes traversals properly, but it is an imperative method
  chain. Powerful, hard to read, hard to optimise.
- **Datalog** (Datomic) is the most uniform model here and handles
  relational and graph in one idea — and is unfamiliar to almost every
  working developer, which rules it out of "very easy" by itself.
- **GraphQL** is not a database query language, despite the name. It is a
  client-server fetching contract with no filtering or aggregation
  semantics of its own. Worth naming only because the confusion is common.

---

## The five failures that repeat

Stripped of syntax, the same defects recur:

| # | Failure | Who | What nova must do |
|---|---|---|---|
| 1 | Reading order ≠ execution order | SQL, SurrealQL | Pipeline. Already decided. |
| 2 | No composition | SQL, Mongo | **Open.** Needs an answer: functions, named pipelines, or nothing. |
| 3 | Invisible rules | SQL three-valued `NULL`, Cypher implicit grouping | Never infer at a distance what the reader cannot see at the call site. |
| 4 | Surface sprawl | SurrealQL, EdgeQL | A hard budget on keywords, enforced by saying no. |
| 5 | Two sublanguages in one | Mongo query vs aggregation, SQL DDL vs DML | One expression language, everywhere, without exception. |

Failure 3 is the one worth dwelling on, because it is the most seductive.
Cypher's implicit grouping and SQL's `NULL` propagation were both added to
make the common case shorter. Both work until the moment they produce a
wrong answer with no error. Every time nova is tempted to infer something,
that is the precedent.

Failure 4 is the one that kills projects quietly. SurrealQL and EdgeQL are
not badly designed — they are *large*, and size is what makes a language
hard, not syntax. The discipline nova needs is not a better grammar, it is
a shorter one.

---

## Sources

- [PRQL](https://prql-lang.org/) · [PRQL/prql on GitHub](https://github.com/PRQL/prql)
- [EdgeDB is now Gel, and Postgres is the Future](https://www.geldata.com/blog/edgedb-is-now-gel-and-postgres-is-the-future)
- [EdgeDB Rebrands as Gel, Brings Full SQL Support](https://linuxiac.com/edgedb-rebrands-as-gel-brings-full-sql-support/)
- [Gel · Database of Databases](https://dbdb.io/db/gel)
