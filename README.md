# novadb

*Français · [English](README.en.md)*

[![CI](https://github.com/FISEM/novadb/actions/workflows/ci.yml/badge.svg)](https://github.com/FISEM/novadb/actions/workflows/ci.yml)

**Relationnel, document et graphe — un seul moteur, un seul pipeline.** Des
collections typées quand tu veux de la structure, sans schéma quand tu n'en
veux pas, et des liens que tu peux suivre. Pas trois styles de requête
boulonnés ensemble : les mêmes étapes, dans le même ordre, sur les trois.

```
person
    where age > 30
    keep following knows
    sort age down
    show name, age
```

Ça se lit de haut en bas, et c'est l'ordre dans lequel ça se passe. Aucun
ordre de clauses à retenir, rien qui s'écrive en premier et s'exécute en
dernier.

**Le langage s'appelle shutup.** Ce n'est pas un accident : c'est ce que la
requête fait au cérémonial. Pas de `SELECT`, pas de `FROM`, pas de
`GROUP BY … HAVING` — tu nommes une collection et tu dis quoi en faire, une
étape par ligne.

## L'essayer dans ton navigateur

`playground/` contient le vrai moteur compilé en WebAssembly. Aucune
installation, aucun serveur, rien ne quitte la page.

```sh
cd playground && python3 -m http.server 8080
```

C'est un dossier statique, donc n'importe quel hébergeur fait l'affaire.
Voir [playground/README.md](playground/README.md).

## Le lancer comme serveur

```sh
cargo run -p server -- --data-file demo.redb --bind 127.0.0.1:8801

curl -X POST http://127.0.0.1:8801/run --data-binary '
person
    where age > 30
    show name, age
'
```

Ou utiliser le client fourni, qui affiche les enregistrements en tableau et
pointe tes fautes du doigt :

```sh
cargo run -p cli -- --url http://127.0.0.1:8801
```

```
shutup> person | take
                     ^
'take' needs a count after it.
Write a whole number, like 'take 10'.
```

## L'essayer en 30 secondes

Il te faut la chaîne d'outils Rust ([rustup.rs](https://rustup.rs) si tu ne
l'as pas).

```sh
git clone https://github.com/FISEM/novadb.git && cd novadb
cargo test --workspace
```

232 tests, aucune donnée de test à préparer. C'est le chemin le plus rapide
pour voir ce que le langage sait faire :
[`crates/engine/tests`](crates/engine/tests) se lit comme une visite guidée.

## Tout le langage

Douze étapes, trois instructions, cinq mots pour compter. Si un mot a besoin
d'un glossaire, c'est le mauvais mot.

| Étape | Fait quoi |
|---|---|
| `where` | garde les enregistrements qui correspondent |
| `show` | garde, renomme ou calcule des champs |
| `sort` | met en ordre — `up` par défaut, `down` pour inverser |
| `take` / `skip` | garde / jette les premiers |
| `unique` | supprime les doublons |
| `join … on …` | apparie avec des enregistrements d'ailleurs |
| `group by` | découpe le flux en groupes |
| `follow` / `keep following` | suit un lien d'un pas / jusqu'au bout |
| `set` / `delete` | modifie / supprime des enregistrements |

`add` insère des enregistrements, `define` déclare une forme ou nomme un
pipeline, `remove` jette une collection entière. Pour compter : `count()`,
`total(x)`, `average(x)`, `lowest(x)`, `highest(x)`.

Les expressions sont celles de Python — `and` / `or` / `not`, `in`,
`is None`, les comparaisons chaînées comme `18 < age < 65`, `len(name)`,
`name.upper()` — et la truthiness aussi. `None` est une valeur ordinaire :
`None == None` est vrai, et il n'y a pas de logique ternaire à garder en
tête.

Référence complète : [docs/language.md](docs/language.md).

### Trois formes, un pipeline

```
# relationnel — collections typées
define person { id: number key, name: string, age: number }
person | where age > 30 | show name

# document — pas de corps, donc chaque enregistrement peut différer
define session
add session { device: "mobile", cart: 3 }
add session { note: "une forme complètement différente" }

# graphe — les liens vivent dans une collection ordinaire, lisible
person | where name == "alice" | keep following knows | show name
```

### Supprimer, c'est la même requête plus une ligne

```
person | where age < 18            # tu les regardes
person | where age < 18 | delete   # tu supprimes exactement ceux-là
```

En SQL tu réécris un `SELECT` en `DELETE` et tu pries pour que le `WHERE`
ait survécu à la modification. Ici la requête destructrice *est* la requête
sûre, avec une étape en plus.

### Il refuse plutôt que de répondre faux

```
person | show name | sort age
'age' was dropped by an earlier 'show', which kept only name.
Move this step above the show, or add age to it.
```

Chaque erreur nomme la chose, explique en une phrase complète, et indique
le correctif. Renvoyer une liste vide en silence, c'est précisément l'échec
que ce langage existe pour éviter.

## Pourquoi c'est fait comme ça

**Le pipeline, pas la liste de clauses.** Le pire défaut de SQL, c'est que
l'ordre de lecture n'est pas l'ordre d'exécution — d'où l'impossibilité de
construire une requête petit à petit, et le fait qu'un alias défini dans le
`SELECT` soit inutilisable dans le `WHERE`. Ici chaque étape prend les
enregistrements que l'étape du dessus a produits. Ce seul choix supprime
`HAVING` (c'est un `where` après un `show`), supprime `COALESCE` (`a or b`
renvoie déjà la première valeur présente), et donne à une collection
document et à une traversée de graphe la même forme qu'à un parcours de
table.

**Petit, exprès.** Ce qui rend SurrealQL et EdgeQL difficiles, ce n'est pas
leur syntaxe, c'est leur taille. Le vocabulaire ci-dessus est le langage
entier, et le garder aussi court est une décision défendue requête par
requête dans [docs/design-notes.md](docs/design-notes.md), avec ce qui a été
rejeté et pourquoi.

**Rien n'est deviné.** Cypher déduit ta clé de regroupement des termes de la
projection qui ne sont pas des agrégats, donc ajouter un champ change
silencieusement le sens de la requête. Ici la clé est ce que tu as écrit
après `group by`, et nulle part ailleurs.

**Pas de protocole réseau.** `POST /run` en HTTP simple, du JSON dans les
deux sens — `curl` est un client. Parler le protocole Postgres est un gros
projet orthogonal qui n'ajoute rien à ce qui rend ce moteur utile.

## Architecture

| Crate | Rôle |
|---|---|
| [`lang`](crates/lang/src) | shutup : analyse lexicale, analyse syntaxique, arbre |
| [`storage`](crates/storage/src) | stockage clé-valeur sur [redb](https://github.com/cberner/redb) ; les enregistrements sont du JSON |
| [`engine`](crates/engine/src) | exécute un pipeline sur le stockage |
| [`server`](crates/server/src) | serveur HTTP (axum) |
| [`cli`](crates/cli/src) | invite de commande par-dessus HTTP |
| [`wasm`](crates/wasm/src) | le moteur dans un navigateur |

## État

Jeune et mononœud : pas d'authentification, pas de cluster, pas d'index
secondaires (un parcours complet est derrière chaque requête, et `join` est
encore une boucle imbriquée). Une forme est une déclaration, pas une
contrainte — `define person { name: string }` n'empêchera pas
`add person { nickname: "al" }` — ce qui est à la fois ce qui rend les
collections document possibles et ce qui fait d'un `define` typé une
documentation plutôt qu'une garantie.

Pas encore exécutés, bien qu'ils se parsent : `define … as` pour nommer un
pipeline, et les collections intégrées `collections` / `fields` / `queries`.
L'ancien frontal SQL est toujours dans l'arbre, avec ses propres tests,
jusqu'à ce qu'ils arrivent et qu'il puisse partir.

À prendre comme un prototype contre lequel construire et qu'on peut casser,
pas comme une base de production.

## Licence

MIT.
