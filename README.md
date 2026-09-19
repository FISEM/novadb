# novadb

*Français · [English](README.en.md)*

[![CI](https://github.com/FISEM/novadb/actions/workflows/ci.yml/badge.svg)](https://github.com/FISEM/novadb/actions/workflows/ci.yml)

Base de données relationnelle, document et graphe dans un seul moteur, avec
son propre langage de requête : **shutup**. Tout est un flux
d'enregistrements, et chaque étape prend ce que l'étape du dessus a produit.

```
person
    where age > 30
    keep following knows
    sort age down
    show name, age
```

Ça se lit de haut en bas, et c'est l'ordre dans lequel ça s'exécute.
L'indentation est le pipeline ; le `|` fait la même chose sur une ligne.

## Essayer

Dans un navigateur, sans rien installer — `playground/` contient le moteur
compilé en WebAssembly :

```sh
cd playground && python3 -m http.server 8080
```

En serveur :

```sh
cargo run -p server -- --data-file demo.redb --bind 127.0.0.1:8801
curl -X POST http://127.0.0.1:8801/run --data-binary 'person | where age > 30'
```

En ligne de commande : `cargo run -p cli -- --url http://127.0.0.1:8801`

Ou simplement `cargo test --workspace` : 232 tests, aucune donnée à préparer.

## Le langage

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

`add` insère, `define` déclare une forme ou nomme un pipeline, `remove` jette
une collection. Pour compter : `count()`, `total(x)`, `average(x)`,
`lowest(x)`, `highest(x)`. Les expressions et la truthiness sont celles de
Python ; `None == None` est vrai.

```
# relationnel
define person { id: number key, name: string, age: number }

# document — pas de corps, chaque enregistrement peut différer
define session
add session { device: "mobile", cart: 3 }

# graphe — les liens sont dans une collection ordinaire
person | where name == "alice" | keep following knows | show name
```

Supprimer, c'est la requête que tu viens de lire, plus une étape :

```
person | where age < 18
person | where age < 18 | delete
```

Et lire un champ qu'un `show` a supprimé lève une erreur au lieu de renvoyer
une liste vide :

```
person | show name | sort age
'age' was dropped by an earlier 'show', which kept only name.
Move this step above the show, or add age to it.
```

Référence complète : [docs/language.md](docs/language.md). Les raisons
derrière chaque choix, et ce qui a été rejeté :
[docs/design-notes.md](docs/design-notes.md) et
[docs/prior-art.md](docs/prior-art.md).

## Architecture

| Crate | Rôle |
|---|---|
| [`lang`](crates/lang/src) | shutup : analyse lexicale et syntaxique |
| [`storage`](crates/storage/src) | clé-valeur sur [redb](https://github.com/cberner/redb), enregistrements JSON |
| [`engine`](crates/engine/src) | exécute un pipeline |
| [`server`](crates/server/src) | serveur HTTP (axum) |
| [`cli`](crates/cli/src) | invite de commande |
| [`wasm`](crates/wasm/src) | le moteur dans un navigateur |

## État

Prototype. Mononœud, pas d'authentification, pas de cluster, pas d'index
(chaque requête fait un parcours complet et `join` est une boucle
imbriquée). Une forme déclarée n'est pas vérifiée : `define person { name:
string }` n'empêche pas `add person { nickname: "al" }`.

`define … as` et les collections intégrées `collections` / `fields` /
`queries` se parsent mais ne s'exécutent pas encore. L'ancien frontal SQL
est toujours dans l'arbre avec ses tests.

## Licence

MIT.
