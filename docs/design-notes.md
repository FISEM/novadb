# nova — design notes

> Why nova is the way it is. The reference is [language.md](language.md);
> this is the argument behind it, kept so the reasoning outlives the
> decisions. Nothing here is implemented yet.

nova replaces SQL in novadb.

It has one design goal, and every decision below answers to it:

> **A sixteen-year-old who has seen a little Python should read a nova query
> and say what it does, without being taught the language first.**

And one rule that enforces it:

> **Plain American English only. Any word a reader cannot recognize without
> prior knowledge is rejected** — no abbreviations, no database jargon, no
> symbols that need explaining.

That rule costs keystrokes and buys readers. Where a shorter, cleverer form
would need explaining, nova takes the longer form that does not.

---

## 1. One idea

**Everything is a stream of records. Every step takes the records from the
step above it and passes records to the step below.**

```
person
    where age > 30
    sort age
    take 10
    show name, age
```

All the people · the ones over thirty · youngest first · the first ten ·
their name and age.

Nobody has to explain that. There is no clause order to memorize, nothing
written first that runs last. You read it top to bottom and that is the
order it happens.

A **record** is a document: field names to values. A collection is a source
of records. That is the whole data model — a typed table is just a
collection whose records agree on their shape.

### What this buys, beyond readability

Delete the last step and you get a preview of what the last step would have
done:

```
person
    where age < 18        # look at them

person
    where age < 18
    delete                # now delete exactly those
```

The destructive query is the safe query plus one line. In SQL you rewrite
`SELECT` into `DELETE` and hope the `WHERE` survived the edit.

---

## 2. Indentation

**Indentation means "this belongs to the line above."**

That is the only structural rule in the language, and it is the same rule
everywhere: pipeline steps, record bodies, shape bodies.

```
person                     define person              add person
    where age > 30             id: number key             id: 1
    show name                  name: string               name: "alice"
```

**Anything indented can be written on one line instead.** Steps separate
with `|`, fields separate with `,` inside `{ }`:

```
person | where age > 30 | show name
define person { id: number key, name: string }
add person { id: 1, name: "alice" }
```

Same grammar, not a second language. The one-line form exists because a
query has to survive being typed into a prompt, pasted into a `curl` body,
or written inside a string in Python or JavaScript — places where a leading
indent is impossible or already means something to the host language.

Indent with spaces or a tab, consistently within a query. A pipeline is flat
by construction, so in practice there is exactly one level of indentation,
and none of Python's deep-nesting pain applies.

---

## 3. The words

The complete vocabulary. If a word here needs a glossary, it is the wrong
word.

| Step | Does |
|---|---|
| `where` | keeps the records that match |
| `show` | keeps only these fields, renames them, computes new ones |
| `sort` | puts them in order — `up` by default, `down` to reverse |
| `take` | keeps the first few |
| `skip` | drops the first few |
| `unique` | drops repeats |
| `join … on …` | pairs each record with matching records from elsewhere |
| `group by` | splits the stream into groups |
| `follow` | walks a link to other records |
| `keep following` | walks that link as far as it goes |
| `set` | changes fields |
| `delete` | removes records |

| Statement | Does |
|---|---|
| `add` | puts new records in |
| `define` | states what shape a collection's records have, or names a piece of a pipeline |
| `remove` | throws a whole collection away |

| Counting | Does |
|---|---|
| `count()` | how many |
| `total(x)` | all of them added up |
| `average(x)` | the average |
| `lowest(x)` / `highest(x)` | the smallest and largest |

| Type | Holds |
|---|---|
| `number` | any number |
| `string` | text |
| `boolean` | `True` or `False` |
| `any` | whatever you put there |

These four are not chosen for plainness but for recognition: they are the
names JavaScript, TypeScript, Java, C#, Go, Kotlin, Swift and Dart already
use. A reader who has met one language has met all four. `text` was the
first draft and is wrong — it is SQL's word and almost nothing else's.

Words that were considered and rejected: `select` (does not say "only
these fields"), `desc` and `avg` and `min` and `max` and `int` and `str`
(abbreviations), `distinct` and `drop` and `insert` (database jargon),
`*` for "as far as it goes" (a symbol needing a lesson), and `text` and
`str` for `string` (one is SQL-only, the other is an abbreviation).

---

## 4. Reading

A bare collection name is already a query: every record in it.

```
person
```

### `where`

```
person
    where age > 30
    where name.startswith("a")      # several where steps just stack
```

```
person | where 18 < age < 65                    # chained, as in Python
person | where device in ["mobile", "tablet"]
person | where nickname is None
```

### `show`

```
person | show name
person | show name, age
person | show name, years: age                  # rename
person | show name, adult: age >= 18            # compute
```

Without `show`, whole records pass through. `show` never has to come first,
so a query is written in the order you think of it.

### `sort`, `take`, `skip`

```
person | sort age                               # youngest first
person | sort age down                          # oldest first
person | sort dept, age down                    # ties broken left to right
person | sort age | take 10
person | sort age | skip 10 | take 10           # page two
```

`sort` sees records as they reach it, so it can sort on a field a later
`show` drops. This is deliberate. It is exactly the bug SQL's clause
ordering invites — and that novadb's own engine shipped with until it was
found and fixed.

### `unique`

```
person | show dept | unique
```

---

## 5. Combining and grouping

### `join`

```
person
    join pet on pet.owner_id == person.id
    show person.name, pet: pet.name
```

```
person
    join pet on pet.owner_id == person.id keep all
    show person.name, pet: pet.name
```

`keep all` keeps people who have no pet, with `pet.*` reading as `None`.
Without it, they are dropped.

### `group by`

`group by` splits the stream into groups. The `show` after it sees each
group as one record, and the counting words fold it:

```
pet
    group by species
    show species, n: count()
```

```
pet
    group by species
    show species, oldest: highest(age), usual: average(age)
    where n > 1
```

Filtering groups is just `where` after the `show`. There is nothing to learn
called `HAVING`, because there is nothing left for it to do.

**Nothing is inferred.** Cypher decides your grouping key by looking at
which parts of the projection are not counting functions, so adding a field
silently changes what the query means. In nova the grouping key is what is
written after `group by`, and nowhere else.

---

## 6. Following links

Links live in an ordinary collection you control.

```
person
    where id == 1
    follow knows
    show name
```

```
person | where id == 1 | follow knows | follow knows | show name
person | where id == 1 | keep following knows | show name
person | where id == 1 | follow knows backward | show name
```

`follow knows` takes one step. `keep following knows` goes as far as the
links go — breadth-first, each record visited once, stopping on loops.
`backward` goes against the arrow.

`follow` is a step like any other, so it composes:

```
person
    where id == 1
    keep following knows
    where age > 30
    sort name
    show name
```

---

## 7. Writing

### `add`

```
add person
    id: 1
    name: "alice"
    age: 30
```

```
add person { id: 2, name: "bob" }
```

### `set` and `delete` — last steps

```
person | where id == 1 | set age = 31
person | where id == 1 | set age = age + 1, seen: True
person | where age < 18 | delete
person | delete                                 # every record, said plainly
```

A `set` or `delete` with no `where` above it is not a mistake the language
hides. You wrote a pipeline over the whole collection, and it reads that
way.

### `define` — stating a shape

```
define person
    id: number key
    name: string
    age: number?
```

```
define session          # no body: any shape, every record different
```

Types: `number`, `string`, `boolean`, `any`. A trailing `?` means the
field may be missing or `None`. `key` marks the field that identifies a
record.

There is one `number`, not an integer and a float. The engine stores JSON
numbers, where that distinction does not exist — so inventing it in the
language would be a lie about what happens.

A body is a claim about shape; no body is no claim. Document collections
are not a special case, they are the absence of one.

### `remove`

```
remove person
remove person if exists
```

---

## 8. Naming a piece of a pipeline

SQL's worst structural failure is that it has no way to reuse a query
except copy and paste. nova answers it with one idea and no new concepts
in the pipeline itself.

```
define adults as
    person
        where age >= 18
```

`adults` is now used exactly the way `person` is:

```
adults | where city == "Paris" | sort name
```

### A name can start with a step instead of a collection

Then it is a piece you pipe records into:

```
define recent as
    where created > 1000
    sort created down
```

```
session | recent | take 10
person  | recent | take 10
```

**The whole rule:** a name that starts with a collection is a source; a name
that starts with a step is something you pipe into. The text says which, so
there is nothing to remember — and when you get it wrong the error teaches
the rule:

```
recent
^^^^^^
'recent' starts with 'where', so it needs records coming in.
Try: person | recent
```

```
person | adults
         ^^^^^^
'adults' already starts with 'person'. Use it on its own:
adults | where ...
```

### There are no parameters, and the pipeline is why

In SQL, reusing a query with a different value forces you into parameterized
views — which means functions, arguments and scope, and a language twice the
size. Here you compose by adding a step:

```
adults | where age > 30
```

That covers nearly everything a parameter would have been for. Parameters
are the obvious next feature and the obvious way to double the size of this
language, so they wait until something real cannot be written without them.

### What a name is, exactly

- **It is re-read, never stored.** `adults` runs its pipeline every time,
  against the records that exist at that moment. There is no stale copy,
  because there is no copy.
- **It shares one list of names with collections.** `define adults as …`
  and a collection called `adults` cannot both exist, because `adults` in a
  query has to mean one thing.
- **It cannot lead back to itself.** `define a as b | …` and
  `define b as a | …` is refused when the second is written, not discovered
  as a hang.
- **`remove` throws it away**, the same word collections use.

---

## 9. The database describes itself

Everything novadb knows about itself is a collection, read with the same
words as your own data.

```
collections
```

One record per collection:

| Field | Holds |
|---|---|
| `name` | what the collection is called |
| `shaped` | `True` if its `define` had a body |
| `records` | how many records it holds |

```
fields
```

One record per field a `define` declared:

| Field | Holds |
|---|---|
| `collection` | which collection it belongs to |
| `name` | what the field is called |
| `type` | `number`, `string`, `boolean` or `any` |
| `optional` | `True` if it was written with `?` |
| `key` | `True` if it identifies the record |

```
queries
```

One record per name defined with `define … as`: its `name`, and the `body`
it stands for.

So the questions every database answers with its own special commands are
just queries here:

```
collections | sort records down | take 5

fields
    where collection == "person"
    show name, type

collections
    join fields on fields.collection == collections.name
    group by collections.name
    show name, how_many: count()
```

They describe themselves, too, which is how you find out what is in them
without reading this page:

```
fields | where collection == "collections"
```

**No new words.** `SHOW TABLES`, `DESCRIBE`, `information_schema` — none of
them exist here, because none of them are separate things. An admin screen
is a query. A migration tool is a query. This section adds nothing to
section 3's vocabulary, and that is the entire point of it.

### They are read-only

`add`, `set` and `delete` do not work on them. Changing a collection's shape
by writing to a record about that shape would be clever, unreadable, and an
excellent way to destroy a database with a typo. `define` and `remove` stay
the only way.

```
add collections { name: "person" }
    ^^^^^^^^^^^
collections is how the database describes itself, so it cannot be written to.
Use 'define person' to make a collection.
```

### Their names are taken

```
define collections
       ^^^^^^^^^^^
'collections' is a built-in name. Pick a different one.
```

### One thing it does not answer yet

For a collection defined with no body, `fields` is empty — though its
records plainly have fields. Reporting the fields records actually carry
means reading every record, which is expensive and surprising for something
that looks like a lookup. Left out of v0 rather than made slow and quiet.

---

## 10. Expressions

Python's, as far as they go.

| | |
|---|---|
| Arithmetic | `+` `-` `*` `/` `%` |
| Comparison | `==` `!=` `<` `<=` `>` `>=`, chainable: `18 < age < 65` |
| Logic | `and` `or` `not` |
| Membership | `in`, `not in` |
| Nothing | `is None`, `is not None` |
| Fields | `p.name`, `p.address.city` |
| Indexing | `tags[0]`, `payload["items"]` |

### Literals

```
1     1.5     "text"     'text'     True     False     None
[1, 2, 3]                { a: 1, b: 2 }
```

**Both quote characters make a string.** This is the one place nova breaks
with SQL on purpose. novadb's own README documents SQL's double-quote rule
as the trap newcomers fall into; a language claiming to be easy does not
keep a trap it has already named.

### Built-ins

Python's names:

```
len(name)        name.upper()        name.lower()
name.startswith("a")                 name.endswith("z")
abs(n)           round(n)            number(x)     string(x)
```

There is no `COALESCE`, because `a or b` already returns the first thing
that is there.

### Truthiness

Python's, exactly: `None`, `False`, `0`, `""`, `[]` and `{}` are false,
everything else is true.

```
person | where nickname        # the ones with a nickname that isn't empty
```

Not a new rule to teach: novadb's engine already evaluates truthiness this
way today.

### `None`

`None` is an ordinary value. `None == None` is true. Sorting puts `None`
first.

nova has **no three-valued logic**. In SQL, `NULL = NULL` is neither true
nor false, and `x NOT IN (…)` silently returns nothing when the list holds
one `NULL`. That is the hardest rule in SQL to hold in your head, it
produces wrong answers instead of errors, and novadb's engine already
declines to implement it.

---

## 11. Errors are part of the design

For the sixteen-year-old, error messages matter more than syntax. A language
with an ordinary grammar and excellent errors is easier than the reverse.
These are a contract, not a nice-to-have:

```
person | where aeg > 30
               ^^^
person has no field 'aeg'. Did you mean 'age'?
```

```
person | show name | sort age
                          ^^^
'age' was dropped by 'show name' on the step above.
Move 'sort age' before it, or add age to the show.
```

```
person
    where age > 30
   show name
   ^
This line is indented less than the one above it, but more than 'person'.
Every step in a pipeline lines up at the same depth.
```

Three rules: point at the exact word, say what is wrong in a full sentence,
name the fix.

---

## 12. The sixteen-year-old test

The bar this document is held to. If a query needs a paragraph of
explanation, the design is wrong, not the reader.

```
# Which of my friends' friends are over 30, oldest first?

person
    where name == "alice"
    keep following knows
    where age > 30
    sort age down
    show name, age
```

Six lines. No punctuation to decode, no word that is not ordinary English.
Read it aloud and it is a sentence.

---

## 13. Deliberately not here, yet

Named so the absences are choices, not oversights.

- **Parameters on a name.** Section 8 says why: the pipeline composes by
  adding a step, which is what a parameter would mostly have been for.
- **Queries inside queries.** A pipeline branches only by joining.
- **`add` from a query.** `add archive (session | where ts < 1000)` is
  obviously useful, left out of v0 to keep `add` one shape.
- **Transactions.** Unchanged from today: none.
- **Shape enforcement.** `define` states a shape; nothing rejects a record
  that disagrees. Same as novadb today, same honest caveat.

---

## 14. Open questions

Decided one way here, and reasonably decidable the other.

1. **`remove person` versus `person | delete`.** One throws away the
   collection, the other empties it. They read alike and differ enormously.
   Is `remove` distinct enough, or does this need a longer, uglier,
   safer word?
2. **`keep all` for left joins.** Plain English, but a two-word step in a
   language that otherwise has none.
3. **Counting outside `group by`.** Is `pet | show n: count()` over the
   whole collection allowed, or does counting always need a `group by`?
4. **`sort` before `show`.** The spec says `sort` sees records before the
   projection drops fields. The alternative is to reject it and let the
   error teach the order. Section 11 shows the error either way.
5. **The name.** `nova` collides with the database it queries.
