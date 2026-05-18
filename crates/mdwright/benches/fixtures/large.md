# Categories, rings, and locality

To talk about étale morphisms we need a notion of "space" that is fluent in algebra. The reading guide pointed at the
destination — an algebraic substitute for a covering map. The route there starts on the algebra side: rings, ideals,
quotients, prime ideals, residue fields, localization, tensor products. Some of this is review. All of it carries a
hidden geometric meaning that we will cash in starting in the next file.

One sentence organizes everything:

> A ring is the algebra of functions on a space.

Once you see that, the operations on rings — quotient, localization, tensor product — become operations on spaces.
Algebra and geometry stop being two subjects; they become two views of the same subject. This file sets up the algebra
side of that dictionary so that the geometry side, in file 02, takes care of itself.

A short note on intuition. The material that follows will move between two registers: algebra (rings, ideals, modules)
and the geometry that algebra encodes. We assume the reader is at home in the first register and is meeting the second
for the first time.

#### Aside (Rust + type-theory analogy).

_Skip this paragraph if Rust trait resolution and dependent-type- theory substitution are not part of your background;
nothing later depends on it._

For readers with those backgrounds: a ring matches a trait describing what operations a type supports — the bundle of
`Add`, `Mul`, `Zero`, `One`, with the usual axioms. Ring homomorphisms reverse the direction of geometric morphisms the
same way substitutions `Γ → Δ` in a dependent type theory run opposite to morphisms of contexts. Localization adjusts a
ring the way adding a typeclass instance for one specific value adjusts a type.

We start with a familiar object and look at it slowly.

## Polynomial rings as functions

Take the ring `ℝ[t]` of polynomials in one variable with real coefficients. You have seen this many times. We are going
to look at it carefully because it is the simplest place where the algebra-as-geometry slogan is visible.

A polynomial like `3t² − 5t + 7` does two things at once. As an algebraic expression, it is a string of coefficients
tagged with powers of `t`. As a function, it sends each real number `a ∈ ℝ` to the real number you get by plugging in
`a`:

```text
(3t² − 5t + 7)(2) = 12 − 10 + 7 = 9.
```

So every element of `ℝ[t]` defines a function `ℝ → ℝ`. Two polynomials that define the same function are equal as
polynomials (for `ℝ`, an infinite field, this is true; over finite fields it fails, and we will be careful when it
matters). The collection of all polynomials, with the obvious addition and multiplication of functions, is a ring. We
call this ring `ℝ[t]`.

The slogan in this case reads:

> `ℝ[t]` is the algebra of polynomial functions on the real line `ℝ`.

The same idea works in more variables. The ring `ℝ[x, y]` is the algebra of polynomial functions of two real variables,
defined on the plane `ℝ²`. The ring `ℝ[x, y, z]` is the algebra of polynomial functions on three-dimensional space `ℝ³`.
And so on. In each case, the ring is "what you can compute pointwise" — the operations of the ring are operations on
functions, computed pointwise.

A few other rings we will use, with their geometric reading.

```text
ℤ        — functions on the geometric object Spec ℤ
ℝ[t]     — polynomial functions on the real line
ℂ[t]     — polynomial functions on the complex line
ℝ[x, y]  — polynomial functions on the real plane
ℝ[x, y]/(x² + y² − 1)  — polynomial functions on the unit circle
ℤ/n      — functions on a finite scheme of n points
```

Some of these we have not earned yet. `Spec ℤ` and "scheme of `n` points" are vocabulary from later in this file and
from file 02. `/(x² + y² − 1)` involves taking a quotient, which we have not defined. We will get to all of it. For now,
just notice the pattern: every ring on this list pairs with a geometric object, and the operations of the ring are the
operations on functions on that object.

The discipline of algebraic geometry is to take this pairing seriously. Whatever ring you write down, **there is a
geometric object behind it**. We are going to build the language to make this precise.

## Notation: categories we use

This file assumes the categorical prerequisites listed in the reading guide: objects, morphisms, functors, natural
transformations, universal properties, limits and colimits, pullbacks. We will not redefine them. Four categories appear
throughout the file:

- **Set**: sets and functions.
- **Rng**: commutative rings with `1`, and ring homomorphisms.
- **A-Alg**: commutative `A`-algebras over a fixed ring `A`, and `A`-algebra homomorphisms.
- **A-Mod**: `A`-modules and `A`-linear maps.

The category of schemes, **Sch**, will appear in file 02.

## Ring homomorphisms: a refresher

The slogan pairs each ring with a space. Before we look at structure inside a single ring, we look at the maps that
connect rings to one another — and, by reversal, the maps that will connect their spaces.

A **ring homomorphism** `φ : A → B` is a function from one ring `A` to another ring `B` that respects the ring
operations. Three conditions:

```text
φ(a + a') = φ(a) + φ(a'),
φ(a · a') = φ(a) · φ(a'),
φ(1_A) = 1_B.
```

So `φ` carries sums to sums, products to products, and the unit of `A` to the unit of `B`. (Carrying `0` to `0` follows
automatically from the first condition.)

Three examples to recognize.

**Inclusion.** The inclusion `ℤ → ℝ` of integers into the reals is a ring homomorphism. So is `ℝ → ℂ`. Whenever a
smaller ring sits inside a larger one, the inclusion is a ring homomorphism.

**Evaluation.** Pick a real number `a`. The function

```text
ev_a : ℝ[t] → ℝ,    p(t) ↦ p(a)
```

that evaluates a polynomial at `a` is a ring homomorphism. The sum-of-polynomials evaluates to the sum of the values;
the product-of-polynomials evaluates to the product of the values; the constant polynomial `1` evaluates to `1`. So
`ev_a` respects all three structures.

There is one such homomorphism for every real number. Each "point of `ℝ`" gives a ring homomorphism `ℝ[t] → ℝ`. This is
a hint — the points of the geometric line correspond to ring homomorphisms into `ℝ`.

**Reduction.** Pick an integer `n > 0`. The function

```text
ℤ → ℤ/n,    a ↦ a mod n,
```

that reduces an integer modulo `n` is a ring homomorphism. The target `ℤ/n` is the ring of integers mod `n`, with
operations inherited from `ℤ`.

These three examples — inclusion, evaluation, reduction — cover most of the ring homomorphisms we will meet in this
file.

### A first hint of arrow-flipping

Look at evaluation again. `ev_a : ℝ[t] → ℝ` is a homomorphism **from the ring of functions on `ℝ`** **to the value field
`ℝ`**. Geometrically, what `ev_a` does is: pick the single point `a` of `ℝ`, extract the value of every polynomial
there.

So the ring homomorphism `ev_a : ℝ[t] → ℝ` corresponds, in geometric terms, to a single point `{a} → ℝ`. The ring map
runs from `ℝ[t]` to `ℝ`. The geometric map runs from `{a}` to `ℝ`. Same direction at the geometric level (point to
line), but the ring map runs the opposite way (functions on the line to functions on the point).

This direction-flipping is the central feature of the ring-as-functions dictionary. We will see it many more times. The
slogan that goes with it:

> A ring map `A → B` is the algebraic shadow of a geometric map running `Spec B → Spec A`, with the arrow flipped.

We have not defined `Spec` yet. Hold the slogan; we will return to it.

## Ideals: vanishing on a subset

Maps between rings gave us the first hint of arrow-flipping. They told us nothing yet about what lives inside a single
ring. The algebraic counterpart of a subset of the space turns out to be a particular kind of subset of the ring — the
**ideal**.

The motivation. Suppose we have a ring `A` of functions on some space `X` (think `A = ℝ[x, y]` and `X = ℝ²`). Suppose
`Z ⊆ X` is a subset of the space (think the `x`-axis `{y = 0}` inside `ℝ²`). A natural question: which functions on `X`
vanish identically on `Z`?

For our example: which polynomials in `ℝ[x, y]` vanish on the `x`-axis? The answer is: polynomials with no constant or
`x`-only term — equivalently, polynomials of the form `y · g(x, y)` for some polynomial `g`.

Three things to notice about this collection.

1. The zero polynomial vanishes on the `x`-axis. So the collection contains `0`.
1. If `f` and `f'` both vanish on the `x`-axis, then so does `f + f'`. The collection is closed under addition.
1. If `f` vanishes on the `x`-axis and `h` is **any** polynomial, then `h · f` also vanishes on the `x`-axis (anything
    times zero is zero). The collection is closed under multiplication by arbitrary elements of the ring.

The third property is the surprising one. The collection of "vanishing functions" is closed under multiplication not
just by itself, but by **any** function in the ring. Multiplication by an arbitrary ring element absorbs into the
collection.

This is the algebraic essence of "vanishing on a subset," and we abstract it into a definition.

### The definition

An **ideal** of a ring `A` is a subset `I ⊆ A` satisfying three conditions:

1. `0 ∈ I`.
1. If `a ∈ I` and `a' ∈ I`, then `a + a' ∈ I`.
1. If `a ∈ I` and `h ∈ A`, then `h · a ∈ I`.

An ideal is more than a sub-ring. A sub-ring is closed under sums and products with itself; an ideal is closed under
products with the **entire ambient ring**. That third condition is the whole point.

The geometric model behind this — vanishing on a subset — is the guide for everything we will do with ideals.

### Examples to anchor the definition

In `ℤ`, the multiples of `5`,

```text
(5) := { 5n : n ∈ ℤ } = { …, −10, −5, 0, 5, 10, … },
```

form an ideal. They contain `0`; the sum of two multiples of `5` is a multiple of `5`; any integer times a multiple of
`5` is a multiple of `5`. Geometrically, this is the ideal of "functions on `Spec ℤ`" vanishing at the prime `(5)`.

In `ℝ[x, y]`, the multiples of `y`,

```text
(y) := { y · g(x, y) : g ∈ ℝ[x, y] },
```

form an ideal. It is the ideal of polynomials vanishing on the `x`-axis.

In `ℝ[x, y]`, the multiples of `x² + y² − 1` form an ideal `(x² - y² − 1)`. It is the ideal of polynomials vanishing on
the unit circle (the locus where `x² + y² = 1`).

These three examples follow a pattern. We have a single element of the ring; the ideal is "all multiples of that
element." Such an ideal is called a **principal ideal**, and the notation `(f)` means "the principal ideal generated by
`f`" — that is, `{ h · f : h ∈ A }`.

### Generated by several elements

Sometimes one element is not enough. The ideal generated by elements `f₁, …, f_n ∈ A`,

```text
(f₁, …, f_n) := { h₁ f₁ + … + h_n f_n : h_i ∈ A },
```

is the set of all `A`-linear combinations of the `f_i`. It is the smallest ideal containing all of `f₁, …, f_n`.

Examples.

In `ℤ`, the ideal `(6, 10)` is the set of all integers of the form `6m + 10n`. By the Euclidean algorithm, every such
integer is a multiple of `gcd(6, 10) = 2`, and conversely every multiple of `2` is `6m + 10n` for some `m, n`. So
`(6, 10) = (2)`, the multiples of `2`. In `ℤ`, every ideal turns out to be principal — generated by a single element.

In `ℝ[x, y]`, the ideal `(x, y)` is the set of polynomials of the form `h₁ x + h₂ y`, equivalently the polynomials with
zero constant term. Geometrically, this is the ideal of polynomials vanishing at the origin `(0, 0)`.

In `ℝ[x, y]`, the ideal `(x − 1, y − 2)` is the polynomials vanishing at the point `(1, 2)`.

### One ideal worth flagging

In `ℝ[t]`, take the ideal `(t²) = { t² · g(t) : g ∈ ℝ[t] }`. This is the multiples of `t²`. Geometrically, what does it
cut out?

Naively, it should cut out "where `t² = 0`," which is just `t = 0`. So we might expect `(t²)` to do the same job as
`(t)`. But it does not. The two ideals are different: `t ∈ (t)` but `t ∉ (t²)`. The ideal `(t²)` is strictly smaller
than `(t)`.

Geometrically, `(t²)` is "the origin, with one extra infinitesimal direction." We will see this more carefully in the
section on quotient rings. For now, hold this distinction: ideals can distinguish "the same subset" with different
multiplicities, and that distinction will turn out to encode infinitesimal data.

## Quotient rings

Ideals named the functions that vanish on a subset, but they have not yet given us the ring of functions on the subset
itself. The quotient construction supplies it.

Given a ring `A` and an ideal `I ⊆ A`, the **quotient ring** `A/I` is built in two steps. First, declare two elements
`a, a' ∈ A` to be equivalent (`a ∼ a'`) if `a − a' ∈ I`. Second, take the set of equivalence classes, and inherit the
ring operations from `A`.

Concretely, every element of `A/I` is represented by some `a ∈ A`, and two representatives `a, a'` give the same element
of `A/I` iff their difference is in `I`. We write `[a]` or `a + I` for the equivalence class, or just `a` when context
makes clear we are working in the quotient.

The geometric reading. If `I` is the ideal of functions vanishing on a subset `Z ⊆ X`, then two functions on `X` are
equivalent modulo `I` iff their difference vanishes on `Z` iff they take the same values on `Z`. So `A/I` is "the ring
of functions on `Z`," seen as a ring in its own right.

> **`A/I` is the ring of functions on the subset cut out by `I`.**

The examples we have already met carry over.

**`ℤ/(5) = ℤ/5`.** The integers modulo `5`, with operations inherited from `ℤ`. A finite ring with five elements.
Geometrically, this is "functions on the closed point `(5) ∈ Spec ℤ`," and the five elements are the five possible
"values" a function on that point can take.

**`ℝ[x, y]/(y) = ℝ[x]`.** Setting `y = 0` reduces a polynomial in two variables to a polynomial in one variable.
Geometrically, this is the ring of polynomial functions on the `x`-axis, which is indeed `ℝ[x]`.

**`ℝ[x, y]/(x² + y² − 1)`.** The ring of polynomial functions on the unit circle. Two polynomials are equivalent iff
their difference vanishes on the circle.

### The infinitesimal twist

Now look at `ℝ[t]/(t²)`. Setting `t² = 0` does not collapse `ℝ[t]` all the way down to `ℝ`. Every element of the
quotient is represented by some polynomial in `ℝ[t]`, modulo polynomials divisible by `t²`. After reducing, every
element has the form

```text
a + bt    with    t² = 0,    a, b ∈ ℝ.
```

So `ℝ[t]/(t²)` is a two-dimensional `ℝ`-vector space, with basis `1, t`. Multiplication is
`(a + bt)(a' + b't) = aa' + (ab' + ba')t`, since `t · t = t² = 0`.

The element `t ∈ ℝ[t]/(t²)` is interesting. It is not zero in the quotient (because `t ∉ (t²)`). But it satisfies
`t² = 0`.

An element with this property — nonzero, but raised to a power equals zero — is called **nilpotent**. The ring
`ℝ[t]/(t²)` has a nonzero nilpotent.

What does this look like geometrically? The ideal `(t²)` cuts out "`t² = 0`," which set-theoretically is just `t = 0` —
the origin. But the **ring** `ℝ[t]/(t²)` has more structure than the ring of functions on a single point would have.
Functions on a single point should form a copy of `ℝ`, not of `ℝ ⊕ ℝ · t`.

The extra dimension is "an infinitesimal direction" at the origin. The nilpotent `t` measures "first-order perturbation
away from the origin." This ring will turn out to be the universal model of "a single point with one infinitesimal
direction sticking out," and we will return to it many times.

The take-away for now: an ideal `I` can have a richer ring `A/I` than its set-theoretic vanishing locus would suggest.
Ideals carry more information than just "where things are zero." That extra information is what makes algebraic geometry
richer than naive set theory.

## The kernel of a ring homomorphism

Ideals and ring maps were introduced separately. They are the same notion, seen from two sides. Given a ring
homomorphism `φ : A → B`, the **kernel** is

```text
ker(φ) := { a ∈ A : φ(a) = 0 } ⊆ A.
```

The kernel is always an ideal. It contains `0` (because `φ(0) = 0`); it is closed under addition
(`φ(a + a') = 0 + 0 = 0`); it is closed under multiplication by any `h ∈ A` (`φ(h · a) = φ(h) · 0 = 0`).

So every ring homomorphism `φ : A → B` produces an ideal `ker(φ) ⊆ A`. Conversely, every ideal `I ⊆ A` is the kernel of
the quotient map `A → A/I`. Ideals and "kernels of ring homomorphisms" are the same notion.

The example to remember. The evaluation homomorphism `ev_a : ℝ[t] → ℝ` has kernel "polynomials vanishing at `a`." A
polynomial `p(t)` vanishes at `a` iff `p(t)` is divisible by `t − a` (this is the factor theorem from elementary
algebra). So

```text
ker(ev_a) = (t − a),
```

the principal ideal generated by `t − a`. The first nontrivial example of "ideal = kernel of a ring map."

This identification is the algebraic-geometry version of the factor theorem: vanishing at a point is captured by the
ideal of multiples of `t − a`.

## Prime ideals

Every ideal cuts out something, but not everything an ideal cuts out deserves to be called a point. Two distinguished
classes — **prime** and **maximal** — are the ones that will. We meet primes first; the maximal case will fall out as a
strengthening.

A **prime ideal** of a ring `A` is an ideal `𝔭 ⊊ A` (proper, not the whole ring) satisfying:

> If `a · b ∈ 𝔭`, then `a ∈ 𝔭` or `b ∈ 𝔭`.

Equivalently: the complement `A \ 𝔭` is closed under multiplication. A product of two things outside `𝔭` is again
outside `𝔭`.

The notation `𝔭` is a fraktur "p," typeset in fraktur in printed mathematics texts; we use the Unicode glyph. There is
nothing special about the typography. It is just a letter that flags "this is a prime."

Why "prime"? Because in `ℤ`, the prime ideals are exactly the ones generated by prime numbers (plus the zero ideal). The
definition "if a product is in `𝔭` then one factor is" is the definition of "prime" for a number `p`: if `p | ab` then
`p | a` or `p | b`. The ideal-theoretic version generalizes the number-theoretic one.

### Examples

**`ℤ`.** The prime ideals are `(0)` and `(p)` for each prime number `p`. The ideal `(0)` is prime because if `ab = 0` in
`ℤ` then `a = 0` or `b = 0` (`ℤ` has no zero divisors). The ideal `(p)` is prime because of the elementary primality
property.

What about `(6)`? It is not prime. We have `2 · 3 = 6 ∈ (6)`, but neither `2 ∈ (6)` nor `3 ∈ (6)`. So `(6)` fails the
prime condition. (In fact `(6) = (2) ∩ (3)`, an intersection of two primes.)

**`ℂ[t]`.** The prime ideals are `(0)` and `(t − a)` for each `a ∈ ℂ`. `(0)` is prime because `ℂ[t]` has no zero
divisors. Each `(t − a)` is prime because if a product `pq` is divisible by `t − a`, then evaluation at `a` gives
`p(a) q(a) = 0`, so `p(a) = 0` or `q(a) = 0`, forcing `t − a` to divide `p` or `q`.

What about `(t² − 1) = ((t − 1)(t + 1))`? It is not prime: the product `(t − 1)(t + 1) ∈ (t² − 1)`, but neither
`(t − 1) ∈ (t² − 1)` nor `(t + 1) ∈ (t² − 1)` (each factor has degree 1, while elements of `(t² − 1)` have degree at
least 2 or are zero).

**`ℝ[t]`.** The prime ideals are `(0)`, `(t − a)` for each `a ∈ ℝ`, and `(F)` for each monic irreducible quadratic
`F = t² + bt + c` with `b² − 4c < 0`. The first two should be familiar; the third is new. A monic quadratic with
negative discriminant is irreducible over `ℝ`, and the corresponding ideal is prime by the same argument: if a product
`pq` is divisible by `F`, then irreducibility of `F` forces it to divide `p` or `q`.

### Why the zero ideal is interesting

For an integral domain `A` (a ring with no zero divisors), the ideal `(0)` is prime. This is just unpacking the
definition: if `ab ∈ (0)` then `ab = 0`, so `a = 0` or `b = 0`, so `a ∈ (0)` or `b ∈ (0)`.

Geometrically, the zero ideal corresponds to a special "point" of the geometric object behind `A`. We will call it the
**generic point**. For `Spec ℤ`, the generic point is `(0)`. For `Spec ℂ[t]`, the generic point is `(0)`.

The generic point is "every point at once," in a precise sense: its closure is the whole space. We will see exactly what
this means in the next file.

## Maximal ideals

Primes generalized "if it divides a product, it divides a factor" and gave us a candidate notion of point. The
strengthening promised above — the one that will give us _closed_ points and a residue field — is the maximal ideal.

A **maximal ideal** of a ring `A` is a proper ideal `𝔪 ⊊ A` such that no other proper ideal strictly contains `𝔪`.
Equivalently: if `I` is an ideal with `𝔪 ⊆ I ⊆ A`, then either `I = 𝔪` or `I = A`.

The notation `𝔪` is a fraktur "m." Same as for `𝔭`, just a letter flagging "this is a maximal ideal."

### Equivalent definition: A/𝔪 is a field

Here is the cleanest equivalent characterization.

> An ideal `𝔪 ⊊ A` is maximal iff the quotient ring `A/𝔪` is a **field**.

The proof is short. If `A/𝔪` is a field, every nonzero element of `A/𝔪` is invertible. So if `I` is an ideal strictly
containing `𝔪`, the image of any element of `I \ 𝔪` is a nonzero element of `A/𝔪`, hence invertible. That forces
`I/𝔪 = A/𝔪`, hence `I = A`. Conversely, if no proper ideal strictly contains `𝔪`, take any `a ∈ A \ 𝔪`. The ideal
`(a) + 𝔪` strictly contains `𝔪`, so it must be all of `A`. So `1 = h a + m` for some `h ∈ A` and `m ∈ 𝔪`. Reducing mod
`𝔪` gives `1 = [h] [a]` in `A/𝔪`, so `[a]` is invertible.

The take-away: **maximal ideals correspond to surjections onto fields.** Each maximal ideal `𝔪 ⊂ A` gives a quotient
field `A/𝔪`, and conversely each surjection `A ↠ k` onto a field has a maximal ideal as kernel.

We will call `A/𝔪` the **residue field** at the maximal ideal `𝔪`. The name comes from the geometric reading: think of a
maximal ideal as a "point" of the space behind `A`, and the residue field as "where values at that point live."

### Maximal implies prime

Every maximal ideal is prime. The proof: if `𝔪` is maximal then `A/𝔪` is a field; fields have no zero divisors; so if
`ab = 0` in `A/𝔪` then `a = 0` or `b = 0`, which is exactly the prime condition for `𝔪`.

The converse fails. The ideal `(0) ⊂ ℤ` is prime (`ℤ` is an integral domain) but not maximal (`ℤ/(0) = ℤ`, which is not
a field). So the prime ideals of `ℤ` strictly contain the maximal ones: every `(p)` is both prime and maximal, but `(0)`
is prime without being maximal.

This distinction — primes that are not maximal — is exactly the "generic point" phenomenon. We come back to it.

### Examples

**`ℤ`.** The maximal ideals are `(p)` for each prime number `p`. The residue field at `(p)` is `ℤ/(p) = 𝔽_p`, the finite
field with `p` elements. So each prime number gives a maximal ideal whose residue field is the finite field with that
prime number of elements.

**`ℂ[t]`.** The maximal ideals are `(t − a)` for each `a ∈ ℂ`. The residue field at `(t − a)` is `ℂ[t]/(t − a) = ℂ`. The
isomorphism `ℂ[t]/(t − a) ≃ ℂ` sends a polynomial to its value at `a` — exactly the evaluation homomorphism `ev_a`.

So in `ℂ[t]`, maximal ideals correspond bijectively to points of `ℂ`. Each maximal ideal `(t − a)` pairs with the point
`a`, and the residue field is `ℂ`. The set of maximal ideals **is** `ℂ`, in disguise.

This is the simplest case where "maximal ideals are points" works literally. Algebra reconstructs the geometry of the
line.

## The riddle of `ℝ[t]`

We can now state the puzzle that motivates the whole construction of algebraic geometry.

In `ℂ[t]`, maximal ideals correspond to points of `ℂ`. Clean.

In `ℝ[t]`, the maximal ideals are of two kinds.

**Real points.** For each `a ∈ ℝ`, the ideal `(t − a)` is maximal, with residue field `ℝ[t]/(t − a) = ℝ`. Each real
number gives a maximal ideal whose residue field is `ℝ`.

**Conjugate-pair points.** For each monic irreducible quadratic `F = t² + bt + c` with `b² − 4c < 0`, the ideal `(F)` is
maximal, with residue field `ℝ[t]/(F) ≃ ℂ`. The isomorphism sends `t` to one of the two conjugate complex roots of `F` —
but symmetrically: there is no algebraic preference between the two roots, so the ideal `(F)` is most naturally
identified with the **pair** of conjugate roots `±i√(c − b²/4) − b/2`.

So the maximal ideals of `ℝ[t]` are **the real numbers** plus **the conjugate pairs of non-real complex numbers**.
Together they cover all the algebraic data the ring `ℝ[t]` "knows about."

The geometry of `ℝ`, on the other hand, only sees the real numbers. The maximal ideals corresponding to conjugate
complex pairs have no counterpart in the real line.

> **The algebra of `ℝ[t]` sees points that the geometry of `ℝ` does not.**

If we want geometry to track the algebra faithfully, we need a notion of "space" that includes the missing conjugate
pairs as honest points. That is what algebraic geometry provides.

## What `Spec` will do

We will define, in the next file, a geometric object `Spec A` attached to every ring `A`. Its **points** will be the
**prime ideals** of `A`. For `ℝ[t]`, the points of `Spec ℝ[t]` will be:

- the real numbers (one point per maximal ideal `(t − a)`),
- the conjugate-pair "points" (one per maximal ideal `(F)` for irreducible quadratic `F` with negative discriminant),
- the generic point `(0)`.

The first two are closed points; the third is the generic point. For any ring, the prime ideals — not just the maximal
ones — make up the points of `Spec A`. The maximal ones are closed points; the others are generic points of various
closed subschemes.

Why include the non-maximal primes? Because they are forced. A ring homomorphism does not, in general, pull back maximal
ideals to maximal ideals; it pulls back primes to primes. So if we want `Spec A` to be functorial in `A`, we have to use
all primes.

That is the construction the next file does. For the rest of this file, we collect the remaining algebra we will need:
modules, localization, tensor products, base change, fibers.

## A second example: `ℤ[t]/(t² − 2)` and its primes

Meet the running example. The ring `B = ℤ[t]/(t² − 2)` will reappear under every lens we build: as a fiber-by-fiber
description in this file, geometrically as `Spec B → Spec ℤ` in file 02, in calculus as the support of `Ω¹` in file 03,
in the étale hierarchy in file 04, and in normalization and Galois theory in file 06. Each return shows the same object
refracted through whatever new structure we have just defined.

As an abelian group, `B` is free on the basis `1, t`, where `t` satisfies `t² = 2`. So every element of `B` looks like
`a + bt` with `a, b ∈ ℤ`. This is "the integers, plus a square root of `2`." It is the ring of integers in the number
field `ℚ(√2)`.

The map `ℤ → B` is the obvious inclusion. Geometrically, it gives a morphism `Spec B → Spec ℤ` (with the arrow flipped,
as always).

We are not going to compute the prime ideals of `B` from scratch. Instead, we will compute the **fibers** of
`Spec B → Spec ℤ` over each prime of `ℤ` once we have the tensor product set up. That is in a few sections. For now,
just register the example: a ring extension `ℤ → ℤ[t]/(t² − 2)`, with the geometric morphism running the other way.

## Where we are, halfway through

We have defined: rings, ring homomorphisms, ideals, quotient rings, kernels, prime ideals, maximal ideals, residue
fields. We have noticed that maximal ideals look like "points" and that the algebra often knows about more points than
the obvious geometry does.

The remaining algebra in this file: modules, localization (zoom into a point), local rings, tensor products (the
algebraic shadow of geometric pullback), base change, and fibers.

After that, the geometry — the actual construction of `Spec A` and the dictionary it builds — is in file 02.

## Modules: vector spaces over a ring

So far, "things over `A`" has meant other rings — `A`-algebras. Many of the constructions ahead want a looser notion:
data carrying an `A`-action, without any multiplication of its own. That notion is the **module**.

An `A`-**module** is an abelian group `M` with a multiplication `A × M → M`, `(a, m) ↦ am`, satisfying:

```text
1 · m = m,
(a + a') m = am + a'm,
a (m + m') = am + am',
(aa') m = a (a' m).
```

A module is a vector space with the field of scalars replaced by a ring. The flexibility lies in what `A` is.

- When `A = k` is a field, an `A`-module is exactly a `k`-vector space.
- When `A = ℤ`, an `A`-module is exactly an abelian group. (The scaling `n · m` is just `m + m + … + m` repeated `n`
    times.)
- When `A = k[t]`, an `A`-module is a `k`-vector space `V` together with a `k`-linear endomorphism `T : V → V`. (The
    scalar `t` acts by `T`; the polynomial `p(t)` acts by `p(T)`.)
- For any ring `A`, the ring `A` is itself an `A`-module (the scaling is just multiplication in `A`).

Three familiar things, all instances of one notion. Modules unify "vector space," "abelian group," and "vector space
with a chosen endomorphism" into a single language.

For us, the most important fact about modules is **finite generation**. An `A`-module `M` is **finitely generated** if
there exist `m₁, …, m_n ∈ M` such that every element of `M` is some `A`-linear combination of them. Equivalently, there
is a surjection `Aⁿ ↠ M`.

Two further adjectives for modules.

A module is **free** of **rank `n`** if `M ≃ Aⁿ`. A free module is a "vector space" over `A` in the most literal sense:
it has a basis.

A module is **flat** if tensoring with it (next section) preserves injections. Flatness is a "good behavior" condition;
we make it precise after we have tensor products. We use it in file 04.

The role of modules in our story. Whenever we have a ring map `A → B`, we can view `B` as an `A`-module (with `A` acting
via the map). Properties like "`B` is finite over `A`" or "`B` is flat over `A`" are then properties of `B` as an
`A`-module, and they control how the geometric morphism `Spec B → Spec A` behaves.

## Localization: zoom in by inverting

Modules let us carry data over a fixed ring. The next operation changes the ring itself, in a way that pictures cleanly
as restriction to an open subset of the underlying space. It is the most overtly geometric of the algebraic operations
in this file.

The motivation. Take `ℝ[x, y]`, the polynomial functions on `ℝ²`. Consider the function `x ∈ ℝ[x, y]`. It vanishes on
the `y`-axis and is nonzero everywhere else. On the open set where `x ≠ 0`, we can divide by `x`. The function `1/x` is
not in `ℝ[x, y]`, but on the open subset `{x ≠ 0}` it makes perfect sense.

What is the right ring of functions on the open subset where `x ≠ 0`? It should be `ℝ[x, y, 1/x]` — polynomial functions
of `x, y, 1/x`, where we have allowed `1/x`.

This is the operation we call **localization**. It builds the smallest ring containing `A` in which a chosen element is
invertible.

### The construction

For a ring `A` and an element `f ∈ A`, the **localization of `A` at `f`** is the set of formal fractions

```text
A[1/f] := { a / fⁿ : a ∈ A, n ≥ 0 } / ∼,
```

with two fractions `a/fⁿ` and `a'/fᵐ` declared equivalent iff there exists `k ≥ 0` such that `fᵏ (fᵐ a − fⁿ a') = 0` in
`A`. The ring operations are the obvious "common denominator" ones.

The natural map `A → A[1/f]` sends `a ↦ a/1`. It is a ring homomorphism, and its image is in the largest piece of
`A[1/f]` that does not need any division by `f`.

The map is universal in the following sense: any ring homomorphism `A → R` that sends `f` to a unit factors uniquely
through `A → A[1/f]`. So `A[1/f]` is "the smallest extension of `A` in which `f` is invertible."

### The geometric reading

We will see in the next file that

> **`A[1/f]` is the ring of functions on the open subset of `Spec A` where `f` does not vanish.**

In other words: localizing the algebra at `f` is the algebraic shadow of restricting `Spec` to the open subset where
`f ≠ 0`.

Three concrete cases to feel.

`ℤ[1/2]` is the ring of "rationals with denominator a power of `2`." Geometrically, it is the ring of functions on the
open subset of `Spec ℤ` complementary to the closed point `(2)`. We have removed the prime `2` from consideration.

`k[t][1/(t − a)]` is the ring of polynomials in `t` with denominators allowed to be powers of `t − a`. Geometrically,
the ring of functions on the open subset of `Spec k[t]` where `t − a ≠ 0`, that is, the affine line minus the point `a`.

`k[x, y][1/x]` is the ring of polynomials in `x, y` with denominators allowed to be powers of `x`. Geometrically, the
ring of functions on the open subset `{x ≠ 0}` of the affine plane — the plane minus the `y`-axis.

### Localization at a multiplicative set

The same construction works for a whole **multiplicatively closed subset** `S ⊆ A` (containing `1` and closed under
products). Define `S⁻¹A` as fractions `a/s` with `a ∈ A`, `s ∈ S`, with the analogous identification. Localization at a
single `f` is the special case `S = {1, f, f², …}`.

Two cases of `S` matter.

`S = {1, f, f², …}` for a single `f ∈ A`: gives `A[1/f]`.

`S = A \ 𝔭` for a prime ideal `𝔭 ⊂ A`: gives **`A_𝔭`**, the **localization of `A` at `𝔭`**. We invert everything not in
`𝔭`.

The second case is the one we use most. It deserves its own section.

## Localization at a prime: zoom in on a point

Take a prime `𝔭 ⊂ A`. The set `S = A \ 𝔭` is multiplicatively closed (because the complement of a prime ideal is closed
under multiplication). The localization `A_𝔭 := S⁻¹A` inverts every element not in `𝔭`.

What does this do? Algebraically: we adjoin inverses to every `s ∈ A` whose image in the residue field `A/𝔭` is nonzero.
We are "throwing in inverses for everything visible at `𝔭`."

The result is striking. **`A_𝔭` is a local ring with exactly one maximal ideal `𝔭 A_𝔭`.** The residue field at this
maximal ideal is `Frac(A/𝔭)`.

The geometric reading:

> **`A_𝔭` is the ring of functions defined on _some_ open neighborhood of `𝔭`, with two functions identified if they
> agree on a smaller neighborhood — equivalently, the stalk of the structure sheaf at `𝔭`.**

Localizing at `𝔭` is "zooming in on the point `𝔭`." Anything that was nonzero at `𝔭` becomes literally invertible.
Anything that was zero at `𝔭` is still in the maximal ideal of the localized ring.

### Examples

For `A = ℤ` and `𝔭 = (5)`:

```text
ℤ_(5) = { m/n ∈ ℚ : 5 ∤ n } = "rationals with denominator coprime to 5".
```

The maximal ideal is `(5) ⊂ ℤ_(5)` (the multiples of `5`). The residue field is `ℤ_(5) / (5) = ℤ/5 = 𝔽₅`.

For `A = ℤ` and `𝔭 = (0)`:

```text
ℤ_(0) = { m/n ∈ ℚ : n ≠ 0 } = ℚ.
```

The localization at the generic point gives back the field of fractions. (The notation `Frac(A)` is the field of
fractions of an integral domain `A`.) Localizing at `(0)` always gives the field of fractions, when `A` is a domain.

## Local rings: the algebra near a point

A **local ring** is a ring with exactly one maximal ideal. The unique maximal ideal we usually call `𝔪`; the residue
field `A/𝔪` we call `κ`.

Most local rings of interest are localizations `A_𝔭`. Other natural local rings include:

- Any field. The maximal ideal is `(0)` and the residue field is the field itself.
- The formal power series ring `k[[t]]` over a field `k`. The maximal ideal is `(t)`, the residue field is `k`.
- The `p`-adic integers `ℤ_p`. The maximal ideal is `(p)`, the residue field is `𝔽_p`.

The slogan to hold:

> A local ring `(𝒪, 𝔪)` is the algebra of an "infinitesimal neighborhood" of one point. The maximal ideal `𝔪` is the
> ideal of functions vanishing at that point. The residue field `κ = 𝒪/𝔪` is where the value at the point lives.

### Local homomorphisms

A **local homomorphism** between local rings is a ring map `(𝒪, 𝔪) → (𝒪', 𝔪')` sending `𝔪` into `𝔪'`. Equivalently, the
preimage of `𝔪'` is `𝔪`. Equivalently, the induced map of residue fields `κ → κ'` is well-defined.

The induced map of residue fields is called the **residue extension**. We will care about whether it is finite,
separable, trivial, and so on.

A great deal of Exposé I is about local homomorphisms. The art is to extract information about the global geometry from
a uniform statement about local homomorphisms.

## Tensor product: combining algebras over a base

Quotient and localization each modified one ring. We still need a way to combine two rings sitting over a common base —
the algebraic shadow of the geometric pullback we will eventually want. That combining operation is the **tensor
product**. We define it patiently because it is the place students most often lose footing.

The setup. Fix a ring `A`. We have two `A`-modules `M` and `N`, and we want a "product" of them over `A`. The cartesian
product `M × N` is **not** the right answer. The cartesian product represents pairs, but most natural constructions on
modules are **bilinear**, not pair-shaped.

A **bilinear** map `M × N → P` is a function that is `A`-linear in each argument separately:

```text
b(am + a'm', n) = a · b(m, n) + a' · b(m', n),
b(m, an + a'n') = a · b(m, n) + a' · b(m, n').
```

Bilinear maps are everywhere. Multiplication on a ring is bilinear. Dot product on a vector space is bilinear. The
pairing `M × Hom(M, P) → P` is bilinear. We want a single algebraic object that "represents" all bilinear maps out of
`M × N`.

That object is the tensor product.

### Definition

The **tensor product** of `M` and `N` over `A`, denoted `M ⊗_A N`, is an `A`-module equipped with a bilinear map

```text
M × N → M ⊗_A N,    (m, n) ↦ m ⊗ n,
```

with the **universal property**: for every bilinear map `b : M × N → P` (with `P` an `A`-module), there is a **unique**
`A`-linear map `b̃ : M ⊗_A N → P` such that `b(m, n) = b̃(m ⊗ n)` for all `m, n`.

Concretely, `M ⊗_A N` is generated by symbols `m ⊗ n` for `m ∈ M`, `n ∈ N`, modulo the bilinearity relations:

```text
(m + m') ⊗ n = m ⊗ n + m' ⊗ n,
m ⊗ (n + n') = m ⊗ n + m ⊗ n',
(am) ⊗ n = m ⊗ (an) = a · (m ⊗ n).
```

The third relation lets `A`-scalars "move freely across the `⊗` symbol." This is what "over `A`" means.

### Bilinearity in practice

The third relation is the load-bearing one. Watch it move a scalar in `M ⊗_ℤ N`:

```text
(2m) ⊗ n  =  2 · (m ⊗ n)  =  m ⊗ (2n).
```

The first equality is the third relation read left-to-right; the second is the same relation read right-to-left, this
time factoring the `2` through the right tensor argument. So `(2m) ⊗ n` and `m ⊗ (2n)` represent the _same_ element of
`M ⊗_ℤ N`.

This is what "scalars move freely across `⊗`" means concretely: a scalar attached to either factor can be moved to the
other, or pulled out front. The same calculation runs over any base ring `A`: a scalar `a ∈ A` can sit on the left of
`⊗`, on the right of `⊗`, or out front, and the three positions represent equal elements.

In practice we never compute inside a tensor product beyond moves like this. We use the universal property and a few
standard formulas.

### Tensor product of `A`-algebras

If `B` and `C` are `A`-algebras (rings with structure maps from `A`), the tensor product `B ⊗_A C` inherits a
multiplication:

```text
(b ⊗ c) · (b' ⊗ c') = (bb') ⊗ (cc').
```

This makes `B ⊗_A C` an `A`-algebra. There are two natural inclusions

```text
B → B ⊗_A C,    b ↦ b ⊗ 1,
C → B ⊗_A C,    c ↦ 1 ⊗ c.
```

Both compose with the structure maps from `A` to give the same map `A → B ⊗_A C`.

The **universal property as a coproduct**: for any `A`-algebra `R` and any pair of `A`-algebra maps `B → R`, `C → R`,
there is a unique `A`-algebra map `B ⊗_A C → R` factoring them. So `B ⊗_A C` is the **coproduct** of `B` and `C` in the
category of `A`-algebras.

This is the operation that, in the next file, will give the fibered product of schemes:

```text
Spec B ×_{Spec A} Spec C = Spec (B ⊗_A C).
```

Geometric pullback, on the algebra side, is the coproduct of algebras, which is the tensor product.

> **Memorize.** Geometric pullback corresponds to _coproduct_ of `A`-algebras — tensor product `B ⊗_A C` — not to the
> category-theoretic pullback of rings. (The categorical pullback in **Rng** is pairs of ring elements with matching
> images, a different operation.) The reason is the arrow-flipping: limits in the category of schemes become colimits in
> the category of rings. This is the place where students slip; we will revisit it in file 02.

### Three patterns to internalize

Almost every concrete tensor-product computation in this file is one of three patterns, each reading the same universal
property through a different presentation of `B`.

Adjoining variables is the first. When `B = A[t₁, …, t_n]` is a polynomial ring, tensoring with `C` carries the
variables along:

```text
B ⊗_A C = C[t₁, …, t_n].
```

The same operation, applied to a quotient, kills the same relations on the other side. If `B = A/I`, then

```text
B ⊗_A C = C / IC,
```

where `IC` is the ideal of `C` generated by the image of `I` under `A → C`. Adding generators on the `B` side and
imposing relations on the `B` side both transfer cleanly to `C`. The pattern is the universal property doing its job: a
map out of `B ⊗_A C` is exactly an `A`-algebra map out of `B` together with a compatible map out of `C`, so whatever
presentation `B` has — generators, relations, or both — gets reproduced over `C`.

The third pattern is the same construction read once more, this time with `B` a localization. If `B = A[1/f]`, then

```text
B ⊗_A C = C[1/f].
```

Tensoring with a localization inverts the same element in `C`.

Three presentations, one universal property. When you see a tensor product, ask which of the three shapes one factor has
— polynomial ring, quotient, or localization — and apply the matching rule.

## Base change: changing the parameter space

The tensor product, once defined, immediately names a geometric operation we have not yet had words for: changing the
parameter space underneath a family. The algebraic name is **base change**. Suppose `A → A'` is a ring map and `B` is an
`A`-algebra. Then

```text
B' := B ⊗_A A'
```

is an `A'`-algebra, with structure map `A' → B'` given by `a' ↦ 1 ⊗ a'`. This `B'` is called the **base change** of `B`
along `A → A'`.

Geometrically: think of `Spec A` as the "parameter space" of a family of geometric objects `Spec B → Spec A`. A morphism
`Spec A' → Spec A` provides a "new parameter space." Base change pulls the family back to the new parameter space.

Two cases of base change matter most.

**Localizing the family.** Take `A → A_𝔭`. Then `B ⊗_A A_𝔭` is "the family `Spec B → Spec A`, viewed near the point
`𝔭`."

**Passing to the fiber.** Take `A → κ(𝔭) = A_𝔭/𝔭A_𝔭`. Then `B ⊗_A κ(𝔭)` is "the **fiber** of the family over the point
`𝔭`."

Both operations are tensor products. The notation `κ(𝔭)` for the residue field `Frac(A/𝔭)` of a prime `𝔭 ⊂ A` will be
standard from now on.

## Fibers, computed

Base change told us that the fiber over a prime is a tensor product with a residue field. The running example was
waiting for exactly this tool. We turn it on now.

Recall the example: `B = ℤ[t]/(t² − 2)` over `A = ℤ`. The fiber of the geometric morphism `Spec B → Spec ℤ` over a prime
`(p) ⊂ ℤ` is computed by base change:

```text
B ⊗_ℤ 𝔽_p = ℤ[t]/(t² − 2) ⊗_ℤ 𝔽_p = 𝔽_p[t]/(t² − 2),
```

using the "killing relations" pattern.

So the fiber over `(p)` is `Spec 𝔽_p[t]/(t² − 2)`, and its structure depends on how `t² − 2` factors over `𝔽_p`. Three
cases.

**`p = 7`.** The squares mod `7` are `{0, 1, 2, 4}` (compute: `3² = 9 ≡ 2 mod 7`). So `2` is a square mod `7`, and
`t² − 2 = (t − 3)(t + 3) mod 7`. The fiber is

```text
𝔽₇[t]/(t² − 2) = 𝔽₇[t]/(t − 3)(t + 3) ≃ 𝔽₇ × 𝔽₇
```

by the Chinese remainder theorem. Two distinct points, each with residue field `𝔽₇`. **Split.**

**`p = 5`.** The squares mod `5` are `{0, 1, 4}`. So `2` is not a square, `t² − 2` is irreducible, and the fiber is

```text
𝔽₅[t]/(t² − 2) ≃ 𝔽_25,
```

a finite field of `25` elements. One point, with residue field `𝔽_25`. **Extension.**

**`p = 2`.** Now `2 ≡ 0 mod 2`, so `t² − 2 ≡ t² mod 2`. The fiber is

```text
𝔽₂[t]/(t² − 2) = 𝔽₂[t]/(t²),
```

the "fat point" with `t² = 0`. One point, with residue field `𝔽₂`, and a nilpotent direction `t` sticking out.
**Ramified.**

The trichotomy:

```text
p = 7:  two distinct points        — split
p = 5:  one point, bigger field    — extension
p = 2:  one point, with nilpotent  — ramified
```

This three-way split is the central rhythm of arithmetic geometry, and the central thread of Exposé I. We will sharpen
it in file 04 into the trichotomy "étale (split or extension) versus ramified."

For now, hold the picture: a single morphism of schemes, three qualitatively different fiber behaviors at three
different primes.

## Noetherian: a finiteness condition

We close with the technical hypothesis that runs underneath all of Exposé I.

A ring `A` is **noetherian** if every ascending chain of ideals `I₁ ⊆ I₂ ⊆ …` is eventually constant. Equivalently:
every ideal of `A` is finitely generated.

Noetherianness rules out infinitely deep nesting of ideals. It is a "no pathologies" condition. Practically every ring
you meet in arithmetic geometry is noetherian: `ℤ`, fields, polynomial rings over fields, quotients and localizations of
these, completions of noetherian local rings. Hilbert's basis theorem says that finitely generated algebras over
noetherian rings are noetherian. The standard nonexample is the polynomial ring in infinitely many variables,
`k[t₁, t₂, t₃, …]`.

The source of Exposé I assumes "all preschemes are locally noetherian" from section I.2 onward. Concretely: every scheme
of interest has a covering by `Spec A`'s with `A` noetherian. This is invisible in practice; we mention it once and move
on.

## Where we have arrived

We have collected the algebra. The slogan that organized everything: a ring is the algebra of functions on a space, with
arrows reversed.

Here is the dictionary so far.

```text
algebra                          geometric meaning (cashed in file 02)
-----------------------          ------------------------------------
ring A                           the affine scheme Spec A
ring map A → B                   morphism Spec B → Spec A (arrow flipped)
ideal I ⊂ A                      a closed subscheme of Spec A
quotient A/I                     ring of functions on the closed subscheme
prime ideal 𝔭 ⊂ A                a point of Spec A
maximal ideal 𝔪 ⊂ A              a closed point of Spec A
residue field κ(𝔭) = Frac(A/𝔭)   the field of values at the point
nilpotent in A                   an infinitesimal direction
localization A[1/f]              open subscheme where f ≠ 0
localization A_𝔭                 local ring near the point 𝔭
tensor product B ⊗_A C           fibered product Spec B ×_{Spec A} Spec C
base change B ⊗_A A'             pulled-back family over Spec A'
B ⊗_A κ(𝔭)                       fiber over 𝔭
```

We started with the riddle that `ℝ[t]` has maximal ideals not visible to the geometry of `ℝ`. We end with a complete
algebraic toolkit. The running example `B = ℤ[t]/(t² − 2)` over `ℤ` showed the trichotomy split / extension / ramified,
which is the central rhythm of the rest of the project.

Geometry is next.


# Categories, rings, and locality

To talk about étale morphisms we need a notion of "space" that is fluent in algebra. The reading guide pointed at the
destination — an algebraic substitute for a covering map. The route there starts on the algebra side: rings, ideals,
quotients, prime ideals, residue fields, localization, tensor products. Some of this is review. All of it carries a
hidden geometric meaning that we will cash in starting in the next file.

One sentence organizes everything:

> A ring is the algebra of functions on a space.

Once you see that, the operations on rings — quotient, localization, tensor product — become operations on spaces.
Algebra and geometry stop being two subjects; they become two views of the same subject. This file sets up the algebra
side of that dictionary so that the geometry side, in file 02, takes care of itself.

A short note on intuition. The material that follows will move between two registers: algebra (rings, ideals, modules)
and the geometry that algebra encodes. We assume the reader is at home in the first register and is meeting the second
for the first time.

#### Aside (Rust + type-theory analogy).

_Skip this paragraph if Rust trait resolution and dependent-type- theory substitution are not part of your background;
nothing later depends on it._

For readers with those backgrounds: a ring matches a trait describing what operations a type supports — the bundle of
`Add`, `Mul`, `Zero`, `One`, with the usual axioms. Ring homomorphisms reverse the direction of geometric morphisms the
same way substitutions `Γ → Δ` in a dependent type theory run opposite to morphisms of contexts. Localization adjusts a
ring the way adding a typeclass instance for one specific value adjusts a type.

We start with a familiar object and look at it slowly.

## Polynomial rings as functions

Take the ring `ℝ[t]` of polynomials in one variable with real coefficients. You have seen this many times. We are going
to look at it carefully because it is the simplest place where the algebra-as-geometry slogan is visible.

A polynomial like `3t² − 5t + 7` does two things at once. As an algebraic expression, it is a string of coefficients
tagged with powers of `t`. As a function, it sends each real number `a ∈ ℝ` to the real number you get by plugging in
`a`:

```text
(3t² − 5t + 7)(2) = 12 − 10 + 7 = 9.
```

So every element of `ℝ[t]` defines a function `ℝ → ℝ`. Two polynomials that define the same function are equal as
polynomials (for `ℝ`, an infinite field, this is true; over finite fields it fails, and we will be careful when it
matters). The collection of all polynomials, with the obvious addition and multiplication of functions, is a ring. We
call this ring `ℝ[t]`.

The slogan in this case reads:

> `ℝ[t]` is the algebra of polynomial functions on the real line `ℝ`.

The same idea works in more variables. The ring `ℝ[x, y]` is the algebra of polynomial functions of two real variables,
defined on the plane `ℝ²`. The ring `ℝ[x, y, z]` is the algebra of polynomial functions on three-dimensional space `ℝ³`.
And so on. In each case, the ring is "what you can compute pointwise" — the operations of the ring are operations on
functions, computed pointwise.

A few other rings we will use, with their geometric reading.

```text
ℤ        — functions on the geometric object Spec ℤ
ℝ[t]     — polynomial functions on the real line
ℂ[t]     — polynomial functions on the complex line
ℝ[x, y]  — polynomial functions on the real plane
ℝ[x, y]/(x² + y² − 1)  — polynomial functions on the unit circle
ℤ/n      — functions on a finite scheme of n points
```

Some of these we have not earned yet. `Spec ℤ` and "scheme of `n` points" are vocabulary from later in this file and
from file 02. `/(x² + y² − 1)` involves taking a quotient, which we have not defined. We will get to all of it. For now,
just notice the pattern: every ring on this list pairs with a geometric object, and the operations of the ring are the
operations on functions on that object.

The discipline of algebraic geometry is to take this pairing seriously. Whatever ring you write down, **there is a
geometric object behind it**. We are going to build the language to make this precise.

## Notation: categories we use

This file assumes the categorical prerequisites listed in the reading guide: objects, morphisms, functors, natural
transformations, universal properties, limits and colimits, pullbacks. We will not redefine them. Four categories appear
throughout the file:

- **Set**: sets and functions.
- **Rng**: commutative rings with `1`, and ring homomorphisms.
- **A-Alg**: commutative `A`-algebras over a fixed ring `A`, and `A`-algebra homomorphisms.
- **A-Mod**: `A`-modules and `A`-linear maps.

The category of schemes, **Sch**, will appear in file 02.

## Ring homomorphisms: a refresher

The slogan pairs each ring with a space. Before we look at structure inside a single ring, we look at the maps that
connect rings to one another — and, by reversal, the maps that will connect their spaces.

A **ring homomorphism** `φ : A → B` is a function from one ring `A` to another ring `B` that respects the ring
operations. Three conditions:

```text
φ(a + a') = φ(a) + φ(a'),
φ(a · a') = φ(a) · φ(a'),
φ(1_A) = 1_B.
```

So `φ` carries sums to sums, products to products, and the unit of `A` to the unit of `B`. (Carrying `0` to `0` follows
automatically from the first condition.)

Three examples to recognize.

**Inclusion.** The inclusion `ℤ → ℝ` of integers into the reals is a ring homomorphism. So is `ℝ → ℂ`. Whenever a
smaller ring sits inside a larger one, the inclusion is a ring homomorphism.

**Evaluation.** Pick a real number `a`. The function

```text
ev_a : ℝ[t] → ℝ,    p(t) ↦ p(a)
```

that evaluates a polynomial at `a` is a ring homomorphism. The sum-of-polynomials evaluates to the sum of the values;
the product-of-polynomials evaluates to the product of the values; the constant polynomial `1` evaluates to `1`. So
`ev_a` respects all three structures.

There is one such homomorphism for every real number. Each "point of `ℝ`" gives a ring homomorphism `ℝ[t] → ℝ`. This is
a hint — the points of the geometric line correspond to ring homomorphisms into `ℝ`.

**Reduction.** Pick an integer `n > 0`. The function

```text
ℤ → ℤ/n,    a ↦ a mod n,
```

that reduces an integer modulo `n` is a ring homomorphism. The target `ℤ/n` is the ring of integers mod `n`, with
operations inherited from `ℤ`.

These three examples — inclusion, evaluation, reduction — cover most of the ring homomorphisms we will meet in this
file.

### A first hint of arrow-flipping

Look at evaluation again. `ev_a : ℝ[t] → ℝ` is a homomorphism **from the ring of functions on `ℝ`** **to the value field
`ℝ`**. Geometrically, what `ev_a` does is: pick the single point `a` of `ℝ`, extract the value of every polynomial
there.

So the ring homomorphism `ev_a : ℝ[t] → ℝ` corresponds, in geometric terms, to a single point `{a} → ℝ`. The ring map
runs from `ℝ[t]` to `ℝ`. The geometric map runs from `{a}` to `ℝ`. Same direction at the geometric level (point to
line), but the ring map runs the opposite way (functions on the line to functions on the point).

This direction-flipping is the central feature of the ring-as-functions dictionary. We will see it many more times. The
slogan that goes with it:

> A ring map `A → B` is the algebraic shadow of a geometric map running `Spec B → Spec A`, with the arrow flipped.

We have not defined `Spec` yet. Hold the slogan; we will return to it.

## Ideals: vanishing on a subset

Maps between rings gave us the first hint of arrow-flipping. They told us nothing yet about what lives inside a single
ring. The algebraic counterpart of a subset of the space turns out to be a particular kind of subset of the ring — the
**ideal**.

The motivation. Suppose we have a ring `A` of functions on some space `X` (think `A = ℝ[x, y]` and `X = ℝ²`). Suppose
`Z ⊆ X` is a subset of the space (think the `x`-axis `{y = 0}` inside `ℝ²`). A natural question: which functions on `X`
vanish identically on `Z`?

For our example: which polynomials in `ℝ[x, y]` vanish on the `x`-axis? The answer is: polynomials with no constant or
`x`-only term — equivalently, polynomials of the form `y · g(x, y)` for some polynomial `g`.

Three things to notice about this collection.

1. The zero polynomial vanishes on the `x`-axis. So the collection contains `0`.
1. If `f` and `f'` both vanish on the `x`-axis, then so does `f + f'`. The collection is closed under addition.
1. If `f` vanishes on the `x`-axis and `h` is **any** polynomial, then `h · f` also vanishes on the `x`-axis (anything
    times zero is zero). The collection is closed under multiplication by arbitrary elements of the ring.

The third property is the surprising one. The collection of "vanishing functions" is closed under multiplication not
just by itself, but by **any** function in the ring. Multiplication by an arbitrary ring element absorbs into the
collection.

This is the algebraic essence of "vanishing on a subset," and we abstract it into a definition.

### The definition

An **ideal** of a ring `A` is a subset `I ⊆ A` satisfying three conditions:

1. `0 ∈ I`.
1. If `a ∈ I` and `a' ∈ I`, then `a + a' ∈ I`.
1. If `a ∈ I` and `h ∈ A`, then `h · a ∈ I`.

An ideal is more than a sub-ring. A sub-ring is closed under sums and products with itself; an ideal is closed under
products with the **entire ambient ring**. That third condition is the whole point.

The geometric model behind this — vanishing on a subset — is the guide for everything we will do with ideals.

### Examples to anchor the definition

In `ℤ`, the multiples of `5`,

```text
(5) := { 5n : n ∈ ℤ } = { …, −10, −5, 0, 5, 10, … },
```

form an ideal. They contain `0`; the sum of two multiples of `5` is a multiple of `5`; any integer times a multiple of
`5` is a multiple of `5`. Geometrically, this is the ideal of "functions on `Spec ℤ`" vanishing at the prime `(5)`.

In `ℝ[x, y]`, the multiples of `y`,

```text
(y) := { y · g(x, y) : g ∈ ℝ[x, y] },
```

form an ideal. It is the ideal of polynomials vanishing on the `x`-axis.

In `ℝ[x, y]`, the multiples of `x² + y² − 1` form an ideal `(x² - y² − 1)`. It is the ideal of polynomials vanishing on
the unit circle (the locus where `x² + y² = 1`).

These three examples follow a pattern. We have a single element of the ring; the ideal is "all multiples of that
element." Such an ideal is called a **principal ideal**, and the notation `(f)` means "the principal ideal generated by
`f`" — that is, `{ h · f : h ∈ A }`.

### Generated by several elements

Sometimes one element is not enough. The ideal generated by elements `f₁, …, f_n ∈ A`,

```text
(f₁, …, f_n) := { h₁ f₁ + … + h_n f_n : h_i ∈ A },
```

is the set of all `A`-linear combinations of the `f_i`. It is the smallest ideal containing all of `f₁, …, f_n`.

Examples.

In `ℤ`, the ideal `(6, 10)` is the set of all integers of the form `6m + 10n`. By the Euclidean algorithm, every such
integer is a multiple of `gcd(6, 10) = 2`, and conversely every multiple of `2` is `6m + 10n` for some `m, n`. So
`(6, 10) = (2)`, the multiples of `2`. In `ℤ`, every ideal turns out to be principal — generated by a single element.

In `ℝ[x, y]`, the ideal `(x, y)` is the set of polynomials of the form `h₁ x + h₂ y`, equivalently the polynomials with
zero constant term. Geometrically, this is the ideal of polynomials vanishing at the origin `(0, 0)`.

In `ℝ[x, y]`, the ideal `(x − 1, y − 2)` is the polynomials vanishing at the point `(1, 2)`.

### One ideal worth flagging

In `ℝ[t]`, take the ideal `(t²) = { t² · g(t) : g ∈ ℝ[t] }`. This is the multiples of `t²`. Geometrically, what does it
cut out?

Naively, it should cut out "where `t² = 0`," which is just `t = 0`. So we might expect `(t²)` to do the same job as
`(t)`. But it does not. The two ideals are different: `t ∈ (t)` but `t ∉ (t²)`. The ideal `(t²)` is strictly smaller
than `(t)`.

Geometrically, `(t²)` is "the origin, with one extra infinitesimal direction." We will see this more carefully in the
section on quotient rings. For now, hold this distinction: ideals can distinguish "the same subset" with different
multiplicities, and that distinction will turn out to encode infinitesimal data.

## Quotient rings

Ideals named the functions that vanish on a subset, but they have not yet given us the ring of functions on the subset
itself. The quotient construction supplies it.

Given a ring `A` and an ideal `I ⊆ A`, the **quotient ring** `A/I` is built in two steps. First, declare two elements
`a, a' ∈ A` to be equivalent (`a ∼ a'`) if `a − a' ∈ I`. Second, take the set of equivalence classes, and inherit the
ring operations from `A`.

Concretely, every element of `A/I` is represented by some `a ∈ A`, and two representatives `a, a'` give the same element
of `A/I` iff their difference is in `I`. We write `[a]` or `a + I` for the equivalence class, or just `a` when context
makes clear we are working in the quotient.

The geometric reading. If `I` is the ideal of functions vanishing on a subset `Z ⊆ X`, then two functions on `X` are
equivalent modulo `I` iff their difference vanishes on `Z` iff they take the same values on `Z`. So `A/I` is "the ring
of functions on `Z`," seen as a ring in its own right.

> **`A/I` is the ring of functions on the subset cut out by `I`.**

The examples we have already met carry over.

**`ℤ/(5) = ℤ/5`.** The integers modulo `5`, with operations inherited from `ℤ`. A finite ring with five elements.
Geometrically, this is "functions on the closed point `(5) ∈ Spec ℤ`," and the five elements are the five possible
"values" a function on that point can take.

**`ℝ[x, y]/(y) = ℝ[x]`.** Setting `y = 0` reduces a polynomial in two variables to a polynomial in one variable.
Geometrically, this is the ring of polynomial functions on the `x`-axis, which is indeed `ℝ[x]`.

**`ℝ[x, y]/(x² + y² − 1)`.** The ring of polynomial functions on the unit circle. Two polynomials are equivalent iff
their difference vanishes on the circle.

### The infinitesimal twist

Now look at `ℝ[t]/(t²)`. Setting `t² = 0` does not collapse `ℝ[t]` all the way down to `ℝ`. Every element of the
quotient is represented by some polynomial in `ℝ[t]`, modulo polynomials divisible by `t²`. After reducing, every
element has the form

```text
a + bt    with    t² = 0,    a, b ∈ ℝ.
```

So `ℝ[t]/(t²)` is a two-dimensional `ℝ`-vector space, with basis `1, t`. Multiplication is
`(a + bt)(a' + b't) = aa' + (ab' + ba')t`, since `t · t = t² = 0`.

The element `t ∈ ℝ[t]/(t²)` is interesting. It is not zero in the quotient (because `t ∉ (t²)`). But it satisfies
`t² = 0`.

An element with this property — nonzero, but raised to a power equals zero — is called **nilpotent**. The ring
`ℝ[t]/(t²)` has a nonzero nilpotent.

What does this look like geometrically? The ideal `(t²)` cuts out "`t² = 0`," which set-theoretically is just `t = 0` —
the origin. But the **ring** `ℝ[t]/(t²)` has more structure than the ring of functions on a single point would have.
Functions on a single point should form a copy of `ℝ`, not of `ℝ ⊕ ℝ · t`.

The extra dimension is "an infinitesimal direction" at the origin. The nilpotent `t` measures "first-order perturbation
away from the origin." This ring will turn out to be the universal model of "a single point with one infinitesimal
direction sticking out," and we will return to it many times.

The take-away for now: an ideal `I` can have a richer ring `A/I` than its set-theoretic vanishing locus would suggest.
Ideals carry more information than just "where things are zero." That extra information is what makes algebraic geometry
richer than naive set theory.

## The kernel of a ring homomorphism

Ideals and ring maps were introduced separately. They are the same notion, seen from two sides. Given a ring
homomorphism `φ : A → B`, the **kernel** is

```text
ker(φ) := { a ∈ A : φ(a) = 0 } ⊆ A.
```

The kernel is always an ideal. It contains `0` (because `φ(0) = 0`); it is closed under addition
(`φ(a + a') = 0 + 0 = 0`); it is closed under multiplication by any `h ∈ A` (`φ(h · a) = φ(h) · 0 = 0`).

So every ring homomorphism `φ : A → B` produces an ideal `ker(φ) ⊆ A`. Conversely, every ideal `I ⊆ A` is the kernel of
the quotient map `A → A/I`. Ideals and "kernels of ring homomorphisms" are the same notion.

The example to remember. The evaluation homomorphism `ev_a : ℝ[t] → ℝ` has kernel "polynomials vanishing at `a`." A
polynomial `p(t)` vanishes at `a` iff `p(t)` is divisible by `t − a` (this is the factor theorem from elementary
algebra). So

```text
ker(ev_a) = (t − a),
```

the principal ideal generated by `t − a`. The first nontrivial example of "ideal = kernel of a ring map."

This identification is the algebraic-geometry version of the factor theorem: vanishing at a point is captured by the
ideal of multiples of `t − a`.

## Prime ideals

Every ideal cuts out something, but not everything an ideal cuts out deserves to be called a point. Two distinguished
classes — **prime** and **maximal** — are the ones that will. We meet primes first; the maximal case will fall out as a
strengthening.

A **prime ideal** of a ring `A` is an ideal `𝔭 ⊊ A` (proper, not the whole ring) satisfying:

> If `a · b ∈ 𝔭`, then `a ∈ 𝔭` or `b ∈ 𝔭`.

Equivalently: the complement `A \ 𝔭` is closed under multiplication. A product of two things outside `𝔭` is again
outside `𝔭`.

The notation `𝔭` is a fraktur "p," typeset in fraktur in printed mathematics texts; we use the Unicode glyph. There is
nothing special about the typography. It is just a letter that flags "this is a prime."

Why "prime"? Because in `ℤ`, the prime ideals are exactly the ones generated by prime numbers (plus the zero ideal). The
definition "if a product is in `𝔭` then one factor is" is the definition of "prime" for a number `p`: if `p | ab` then
`p | a` or `p | b`. The ideal-theoretic version generalizes the number-theoretic one.

### Examples

**`ℤ`.** The prime ideals are `(0)` and `(p)` for each prime number `p`. The ideal `(0)` is prime because if `ab = 0` in
`ℤ` then `a = 0` or `b = 0` (`ℤ` has no zero divisors). The ideal `(p)` is prime because of the elementary primality
property.

What about `(6)`? It is not prime. We have `2 · 3 = 6 ∈ (6)`, but neither `2 ∈ (6)` nor `3 ∈ (6)`. So `(6)` fails the
prime condition. (In fact `(6) = (2) ∩ (3)`, an intersection of two primes.)

**`ℂ[t]`.** The prime ideals are `(0)` and `(t − a)` for each `a ∈ ℂ`. `(0)` is prime because `ℂ[t]` has no zero
divisors. Each `(t − a)` is prime because if a product `pq` is divisible by `t − a`, then evaluation at `a` gives
`p(a) q(a) = 0`, so `p(a) = 0` or `q(a) = 0`, forcing `t − a` to divide `p` or `q`.

What about `(t² − 1) = ((t − 1)(t + 1))`? It is not prime: the product `(t − 1)(t + 1) ∈ (t² − 1)`, but neither
`(t − 1) ∈ (t² − 1)` nor `(t + 1) ∈ (t² − 1)` (each factor has degree 1, while elements of `(t² − 1)` have degree at
least 2 or are zero).

**`ℝ[t]`.** The prime ideals are `(0)`, `(t − a)` for each `a ∈ ℝ`, and `(F)` for each monic irreducible quadratic
`F = t² + bt + c` with `b² − 4c < 0`. The first two should be familiar; the third is new. A monic quadratic with
negative discriminant is irreducible over `ℝ`, and the corresponding ideal is prime by the same argument: if a product
`pq` is divisible by `F`, then irreducibility of `F` forces it to divide `p` or `q`.

### Why the zero ideal is interesting

For an integral domain `A` (a ring with no zero divisors), the ideal `(0)` is prime. This is just unpacking the
definition: if `ab ∈ (0)` then `ab = 0`, so `a = 0` or `b = 0`, so `a ∈ (0)` or `b ∈ (0)`.

Geometrically, the zero ideal corresponds to a special "point" of the geometric object behind `A`. We will call it the
**generic point**. For `Spec ℤ`, the generic point is `(0)`. For `Spec ℂ[t]`, the generic point is `(0)`.

The generic point is "every point at once," in a precise sense: its closure is the whole space. We will see exactly what
this means in the next file.

## Maximal ideals

Primes generalized "if it divides a product, it divides a factor" and gave us a candidate notion of point. The
strengthening promised above — the one that will give us _closed_ points and a residue field — is the maximal ideal.

A **maximal ideal** of a ring `A` is a proper ideal `𝔪 ⊊ A` such that no other proper ideal strictly contains `𝔪`.
Equivalently: if `I` is an ideal with `𝔪 ⊆ I ⊆ A`, then either `I = 𝔪` or `I = A`.

The notation `𝔪` is a fraktur "m." Same as for `𝔭`, just a letter flagging "this is a maximal ideal."

### Equivalent definition: A/𝔪 is a field

Here is the cleanest equivalent characterization.

> An ideal `𝔪 ⊊ A` is maximal iff the quotient ring `A/𝔪` is a **field**.

The proof is short. If `A/𝔪` is a field, every nonzero element of `A/𝔪` is invertible. So if `I` is an ideal strictly
containing `𝔪`, the image of any element of `I \ 𝔪` is a nonzero element of `A/𝔪`, hence invertible. That forces
`I/𝔪 = A/𝔪`, hence `I = A`. Conversely, if no proper ideal strictly contains `𝔪`, take any `a ∈ A \ 𝔪`. The ideal
`(a) + 𝔪` strictly contains `𝔪`, so it must be all of `A`. So `1 = h a + m` for some `h ∈ A` and `m ∈ 𝔪`. Reducing mod
`𝔪` gives `1 = [h] [a]` in `A/𝔪`, so `[a]` is invertible.

The take-away: **maximal ideals correspond to surjections onto fields.** Each maximal ideal `𝔪 ⊂ A` gives a quotient
field `A/𝔪`, and conversely each surjection `A ↠ k` onto a field has a maximal ideal as kernel.

We will call `A/𝔪` the **residue field** at the maximal ideal `𝔪`. The name comes from the geometric reading: think of a
maximal ideal as a "point" of the space behind `A`, and the residue field as "where values at that point live."

### Maximal implies prime

Every maximal ideal is prime. The proof: if `𝔪` is maximal then `A/𝔪` is a field; fields have no zero divisors; so if
`ab = 0` in `A/𝔪` then `a = 0` or `b = 0`, which is exactly the prime condition for `𝔪`.

The converse fails. The ideal `(0) ⊂ ℤ` is prime (`ℤ` is an integral domain) but not maximal (`ℤ/(0) = ℤ`, which is not
a field). So the prime ideals of `ℤ` strictly contain the maximal ones: every `(p)` is both prime and maximal, but `(0)`
is prime without being maximal.

This distinction — primes that are not maximal — is exactly the "generic point" phenomenon. We come back to it.

### Examples

**`ℤ`.** The maximal ideals are `(p)` for each prime number `p`. The residue field at `(p)` is `ℤ/(p) = 𝔽_p`, the finite
field with `p` elements. So each prime number gives a maximal ideal whose residue field is the finite field with that
prime number of elements.

**`ℂ[t]`.** The maximal ideals are `(t − a)` for each `a ∈ ℂ`. The residue field at `(t − a)` is `ℂ[t]/(t − a) = ℂ`. The
isomorphism `ℂ[t]/(t − a) ≃ ℂ` sends a polynomial to its value at `a` — exactly the evaluation homomorphism `ev_a`.

So in `ℂ[t]`, maximal ideals correspond bijectively to points of `ℂ`. Each maximal ideal `(t − a)` pairs with the point
`a`, and the residue field is `ℂ`. The set of maximal ideals **is** `ℂ`, in disguise.

This is the simplest case where "maximal ideals are points" works literally. Algebra reconstructs the geometry of the
line.

## The riddle of `ℝ[t]`

We can now state the puzzle that motivates the whole construction of algebraic geometry.

In `ℂ[t]`, maximal ideals correspond to points of `ℂ`. Clean.

In `ℝ[t]`, the maximal ideals are of two kinds.

**Real points.** For each `a ∈ ℝ`, the ideal `(t − a)` is maximal, with residue field `ℝ[t]/(t − a) = ℝ`. Each real
number gives a maximal ideal whose residue field is `ℝ`.

**Conjugate-pair points.** For each monic irreducible quadratic `F = t² + bt + c` with `b² − 4c < 0`, the ideal `(F)` is
maximal, with residue field `ℝ[t]/(F) ≃ ℂ`. The isomorphism sends `t` to one of the two conjugate complex roots of `F` —
but symmetrically: there is no algebraic preference between the two roots, so the ideal `(F)` is most naturally
identified with the **pair** of conjugate roots `±i√(c − b²/4) − b/2`.

So the maximal ideals of `ℝ[t]` are **the real numbers** plus **the conjugate pairs of non-real complex numbers**.
Together they cover all the algebraic data the ring `ℝ[t]` "knows about."

The geometry of `ℝ`, on the other hand, only sees the real numbers. The maximal ideals corresponding to conjugate
complex pairs have no counterpart in the real line.

> **The algebra of `ℝ[t]` sees points that the geometry of `ℝ` does not.**

If we want geometry to track the algebra faithfully, we need a notion of "space" that includes the missing conjugate
pairs as honest points. That is what algebraic geometry provides.

## What `Spec` will do

We will define, in the next file, a geometric object `Spec A` attached to every ring `A`. Its **points** will be the
**prime ideals** of `A`. For `ℝ[t]`, the points of `Spec ℝ[t]` will be:

- the real numbers (one point per maximal ideal `(t − a)`),
- the conjugate-pair "points" (one per maximal ideal `(F)` for irreducible quadratic `F` with negative discriminant),
- the generic point `(0)`.

The first two are closed points; the third is the generic point. For any ring, the prime ideals — not just the maximal
ones — make up the points of `Spec A`. The maximal ones are closed points; the others are generic points of various
closed subschemes.

Why include the non-maximal primes? Because they are forced. A ring homomorphism does not, in general, pull back maximal
ideals to maximal ideals; it pulls back primes to primes. So if we want `Spec A` to be functorial in `A`, we have to use
all primes.

That is the construction the next file does. For the rest of this file, we collect the remaining algebra we will need:
modules, localization, tensor products, base change, fibers.

## A second example: `ℤ[t]/(t² − 2)` and its primes

Meet the running example. The ring `B = ℤ[t]/(t² − 2)` will reappear under every lens we build: as a fiber-by-fiber
description in this file, geometrically as `Spec B → Spec ℤ` in file 02, in calculus as the support of `Ω¹` in file 03,
in the étale hierarchy in file 04, and in normalization and Galois theory in file 06. Each return shows the same object
refracted through whatever new structure we have just defined.

As an abelian group, `B` is free on the basis `1, t`, where `t` satisfies `t² = 2`. So every element of `B` looks like
`a + bt` with `a, b ∈ ℤ`. This is "the integers, plus a square root of `2`." It is the ring of integers in the number
field `ℚ(√2)`.

The map `ℤ → B` is the obvious inclusion. Geometrically, it gives a morphism `Spec B → Spec ℤ` (with the arrow flipped,
as always).

We are not going to compute the prime ideals of `B` from scratch. Instead, we will compute the **fibers** of
`Spec B → Spec ℤ` over each prime of `ℤ` once we have the tensor product set up. That is in a few sections. For now,
just register the example: a ring extension `ℤ → ℤ[t]/(t² − 2)`, with the geometric morphism running the other way.

## Where we are, halfway through

We have defined: rings, ring homomorphisms, ideals, quotient rings, kernels, prime ideals, maximal ideals, residue
fields. We have noticed that maximal ideals look like "points" and that the algebra often knows about more points than
the obvious geometry does.

The remaining algebra in this file: modules, localization (zoom into a point), local rings, tensor products (the
algebraic shadow of geometric pullback), base change, and fibers.

After that, the geometry — the actual construction of `Spec A` and the dictionary it builds — is in file 02.

## Modules: vector spaces over a ring

So far, "things over `A`" has meant other rings — `A`-algebras. Many of the constructions ahead want a looser notion:
data carrying an `A`-action, without any multiplication of its own. That notion is the **module**.

An `A`-**module** is an abelian group `M` with a multiplication `A × M → M`, `(a, m) ↦ am`, satisfying:

```text
1 · m = m,
(a + a') m = am + a'm,
a (m + m') = am + am',
(aa') m = a (a' m).
```

A module is a vector space with the field of scalars replaced by a ring. The flexibility lies in what `A` is.

- When `A = k` is a field, an `A`-module is exactly a `k`-vector space.
- When `A = ℤ`, an `A`-module is exactly an abelian group. (The scaling `n · m` is just `m + m + … + m` repeated `n`
    times.)
- When `A = k[t]`, an `A`-module is a `k`-vector space `V` together with a `k`-linear endomorphism `T : V → V`. (The
    scalar `t` acts by `T`; the polynomial `p(t)` acts by `p(T)`.)
- For any ring `A`, the ring `A` is itself an `A`-module (the scaling is just multiplication in `A`).

Three familiar things, all instances of one notion. Modules unify "vector space," "abelian group," and "vector space
with a chosen endomorphism" into a single language.

For us, the most important fact about modules is **finite generation**. An `A`-module `M` is **finitely generated** if
there exist `m₁, …, m_n ∈ M` such that every element of `M` is some `A`-linear combination of them. Equivalently, there
is a surjection `Aⁿ ↠ M`.

Two further adjectives for modules.

A module is **free** of **rank `n`** if `M ≃ Aⁿ`. A free module is a "vector space" over `A` in the most literal sense:
it has a basis.

A module is **flat** if tensoring with it (next section) preserves injections. Flatness is a "good behavior" condition;
we make it precise after we have tensor products. We use it in file 04.

The role of modules in our story. Whenever we have a ring map `A → B`, we can view `B` as an `A`-module (with `A` acting
via the map). Properties like "`B` is finite over `A`" or "`B` is flat over `A`" are then properties of `B` as an
`A`-module, and they control how the geometric morphism `Spec B → Spec A` behaves.

## Localization: zoom in by inverting

Modules let us carry data over a fixed ring. The next operation changes the ring itself, in a way that pictures cleanly
as restriction to an open subset of the underlying space. It is the most overtly geometric of the algebraic operations
in this file.

The motivation. Take `ℝ[x, y]`, the polynomial functions on `ℝ²`. Consider the function `x ∈ ℝ[x, y]`. It vanishes on
the `y`-axis and is nonzero everywhere else. On the open set where `x ≠ 0`, we can divide by `x`. The function `1/x` is
not in `ℝ[x, y]`, but on the open subset `{x ≠ 0}` it makes perfect sense.

What is the right ring of functions on the open subset where `x ≠ 0`? It should be `ℝ[x, y, 1/x]` — polynomial functions
of `x, y, 1/x`, where we have allowed `1/x`.

This is the operation we call **localization**. It builds the smallest ring containing `A` in which a chosen element is
invertible.

### The construction

For a ring `A` and an element `f ∈ A`, the **localization of `A` at `f`** is the set of formal fractions

```text
A[1/f] := { a / fⁿ : a ∈ A, n ≥ 0 } / ∼,
```

with two fractions `a/fⁿ` and `a'/fᵐ` declared equivalent iff there exists `k ≥ 0` such that `fᵏ (fᵐ a − fⁿ a') = 0` in
`A`. The ring operations are the obvious "common denominator" ones.

The natural map `A → A[1/f]` sends `a ↦ a/1`. It is a ring homomorphism, and its image is in the largest piece of
`A[1/f]` that does not need any division by `f`.

The map is universal in the following sense: any ring homomorphism `A → R` that sends `f` to a unit factors uniquely
through `A → A[1/f]`. So `A[1/f]` is "the smallest extension of `A` in which `f` is invertible."

### The geometric reading

We will see in the next file that

> **`A[1/f]` is the ring of functions on the open subset of `Spec A` where `f` does not vanish.**

In other words: localizing the algebra at `f` is the algebraic shadow of restricting `Spec` to the open subset where
`f ≠ 0`.

Three concrete cases to feel.

`ℤ[1/2]` is the ring of "rationals with denominator a power of `2`." Geometrically, it is the ring of functions on the
open subset of `Spec ℤ` complementary to the closed point `(2)`. We have removed the prime `2` from consideration.

`k[t][1/(t − a)]` is the ring of polynomials in `t` with denominators allowed to be powers of `t − a`. Geometrically,
the ring of functions on the open subset of `Spec k[t]` where `t − a ≠ 0`, that is, the affine line minus the point `a`.

`k[x, y][1/x]` is the ring of polynomials in `x, y` with denominators allowed to be powers of `x`. Geometrically, the
ring of functions on the open subset `{x ≠ 0}` of the affine plane — the plane minus the `y`-axis.

### Localization at a multiplicative set

The same construction works for a whole **multiplicatively closed subset** `S ⊆ A` (containing `1` and closed under
products). Define `S⁻¹A` as fractions `a/s` with `a ∈ A`, `s ∈ S`, with the analogous identification. Localization at a
single `f` is the special case `S = {1, f, f², …}`.

Two cases of `S` matter.

`S = {1, f, f², …}` for a single `f ∈ A`: gives `A[1/f]`.

`S = A \ 𝔭` for a prime ideal `𝔭 ⊂ A`: gives **`A_𝔭`**, the **localization of `A` at `𝔭`**. We invert everything not in
`𝔭`.

The second case is the one we use most. It deserves its own section.

## Localization at a prime: zoom in on a point

Take a prime `𝔭 ⊂ A`. The set `S = A \ 𝔭` is multiplicatively closed (because the complement of a prime ideal is closed
under multiplication). The localization `A_𝔭 := S⁻¹A` inverts every element not in `𝔭`.

What does this do? Algebraically: we adjoin inverses to every `s ∈ A` whose image in the residue field `A/𝔭` is nonzero.
We are "throwing in inverses for everything visible at `𝔭`."

The result is striking. **`A_𝔭` is a local ring with exactly one maximal ideal `𝔭 A_𝔭`.** The residue field at this
maximal ideal is `Frac(A/𝔭)`.

The geometric reading:

> **`A_𝔭` is the ring of functions defined on _some_ open neighborhood of `𝔭`, with two functions identified if they
> agree on a smaller neighborhood — equivalently, the stalk of the structure sheaf at `𝔭`.**

Localizing at `𝔭` is "zooming in on the point `𝔭`." Anything that was nonzero at `𝔭` becomes literally invertible.
Anything that was zero at `𝔭` is still in the maximal ideal of the localized ring.

### Examples

For `A = ℤ` and `𝔭 = (5)`:

```text
ℤ_(5) = { m/n ∈ ℚ : 5 ∤ n } = "rationals with denominator coprime to 5".
```

The maximal ideal is `(5) ⊂ ℤ_(5)` (the multiples of `5`). The residue field is `ℤ_(5) / (5) = ℤ/5 = 𝔽₅`.

For `A = ℤ` and `𝔭 = (0)`:

```text
ℤ_(0) = { m/n ∈ ℚ : n ≠ 0 } = ℚ.
```

The localization at the generic point gives back the field of fractions. (The notation `Frac(A)` is the field of
fractions of an integral domain `A`.) Localizing at `(0)` always gives the field of fractions, when `A` is a domain.

## Local rings: the algebra near a point

A **local ring** is a ring with exactly one maximal ideal. The unique maximal ideal we usually call `𝔪`; the residue
field `A/𝔪` we call `κ`.

Most local rings of interest are localizations `A_𝔭`. Other natural local rings include:

- Any field. The maximal ideal is `(0)` and the residue field is the field itself.
- The formal power series ring `k[[t]]` over a field `k`. The maximal ideal is `(t)`, the residue field is `k`.
- The `p`-adic integers `ℤ_p`. The maximal ideal is `(p)`, the residue field is `𝔽_p`.

The slogan to hold:

> A local ring `(𝒪, 𝔪)` is the algebra of an "infinitesimal neighborhood" of one point. The maximal ideal `𝔪` is the
> ideal of functions vanishing at that point. The residue field `κ = 𝒪/𝔪` is where the value at the point lives.

### Local homomorphisms

A **local homomorphism** between local rings is a ring map `(𝒪, 𝔪) → (𝒪', 𝔪')` sending `𝔪` into `𝔪'`. Equivalently, the
preimage of `𝔪'` is `𝔪`. Equivalently, the induced map of residue fields `κ → κ'` is well-defined.

The induced map of residue fields is called the **residue extension**. We will care about whether it is finite,
separable, trivial, and so on.

A great deal of Exposé I is about local homomorphisms. The art is to extract information about the global geometry from
a uniform statement about local homomorphisms.

## Tensor product: combining algebras over a base

Quotient and localization each modified one ring. We still need a way to combine two rings sitting over a common base —
the algebraic shadow of the geometric pullback we will eventually want. That combining operation is the **tensor
product**. We define it patiently because it is the place students most often lose footing.

The setup. Fix a ring `A`. We have two `A`-modules `M` and `N`, and we want a "product" of them over `A`. The cartesian
product `M × N` is **not** the right answer. The cartesian product represents pairs, but most natural constructions on
modules are **bilinear**, not pair-shaped.

A **bilinear** map `M × N → P` is a function that is `A`-linear in each argument separately:

```text
b(am + a'm', n) = a · b(m, n) + a' · b(m', n),
b(m, an + a'n') = a · b(m, n) + a' · b(m, n').
```

Bilinear maps are everywhere. Multiplication on a ring is bilinear. Dot product on a vector space is bilinear. The
pairing `M × Hom(M, P) → P` is bilinear. We want a single algebraic object that "represents" all bilinear maps out of
`M × N`.

That object is the tensor product.

### Definition

The **tensor product** of `M` and `N` over `A`, denoted `M ⊗_A N`, is an `A`-module equipped with a bilinear map

```text
M × N → M ⊗_A N,    (m, n) ↦ m ⊗ n,
```

with the **universal property**: for every bilinear map `b : M × N → P` (with `P` an `A`-module), there is a **unique**
`A`-linear map `b̃ : M ⊗_A N → P` such that `b(m, n) = b̃(m ⊗ n)` for all `m, n`.

Concretely, `M ⊗_A N` is generated by symbols `m ⊗ n` for `m ∈ M`, `n ∈ N`, modulo the bilinearity relations:

```text
(m + m') ⊗ n = m ⊗ n + m' ⊗ n,
m ⊗ (n + n') = m ⊗ n + m ⊗ n',
(am) ⊗ n = m ⊗ (an) = a · (m ⊗ n).
```

The third relation lets `A`-scalars "move freely across the `⊗` symbol." This is what "over `A`" means.

### Bilinearity in practice

The third relation is the load-bearing one. Watch it move a scalar in `M ⊗_ℤ N`:

```text
(2m) ⊗ n  =  2 · (m ⊗ n)  =  m ⊗ (2n).
```

The first equality is the third relation read left-to-right; the second is the same relation read right-to-left, this
time factoring the `2` through the right tensor argument. So `(2m) ⊗ n` and `m ⊗ (2n)` represent the _same_ element of
`M ⊗_ℤ N`.

This is what "scalars move freely across `⊗`" means concretely: a scalar attached to either factor can be moved to the
other, or pulled out front. The same calculation runs over any base ring `A`: a scalar `a ∈ A` can sit on the left of
`⊗`, on the right of `⊗`, or out front, and the three positions represent equal elements.

In practice we never compute inside a tensor product beyond moves like this. We use the universal property and a few
standard formulas.

### Tensor product of `A`-algebras

If `B` and `C` are `A`-algebras (rings with structure maps from `A`), the tensor product `B ⊗_A C` inherits a
multiplication:

```text
(b ⊗ c) · (b' ⊗ c') = (bb') ⊗ (cc').
```

This makes `B ⊗_A C` an `A`-algebra. There are two natural inclusions

```text
B → B ⊗_A C,    b ↦ b ⊗ 1,
C → B ⊗_A C,    c ↦ 1 ⊗ c.
```

Both compose with the structure maps from `A` to give the same map `A → B ⊗_A C`.

The **universal property as a coproduct**: for any `A`-algebra `R` and any pair of `A`-algebra maps `B → R`, `C → R`,
there is a unique `A`-algebra map `B ⊗_A C → R` factoring them. So `B ⊗_A C` is the **coproduct** of `B` and `C` in the
category of `A`-algebras.

This is the operation that, in the next file, will give the fibered product of schemes:

```text
Spec B ×_{Spec A} Spec C = Spec (B ⊗_A C).
```

Geometric pullback, on the algebra side, is the coproduct of algebras, which is the tensor product.

> **Memorize.** Geometric pullback corresponds to _coproduct_ of `A`-algebras — tensor product `B ⊗_A C` — not to the
> category-theoretic pullback of rings. (The categorical pullback in **Rng** is pairs of ring elements with matching
> images, a different operation.) The reason is the arrow-flipping: limits in the category of schemes become colimits in
> the category of rings. This is the place where students slip; we will revisit it in file 02.

### Three patterns to internalize

Almost every concrete tensor-product computation in this file is one of three patterns, each reading the same universal
property through a different presentation of `B`.

Adjoining variables is the first. When `B = A[t₁, …, t_n]` is a polynomial ring, tensoring with `C` carries the
variables along:

```text
B ⊗_A C = C[t₁, …, t_n].
```

The same operation, applied to a quotient, kills the same relations on the other side. If `B = A/I`, then

```text
B ⊗_A C = C / IC,
```

where `IC` is the ideal of `C` generated by the image of `I` under `A → C`. Adding generators on the `B` side and
imposing relations on the `B` side both transfer cleanly to `C`. The pattern is the universal property doing its job: a
map out of `B ⊗_A C` is exactly an `A`-algebra map out of `B` together with a compatible map out of `C`, so whatever
presentation `B` has — generators, relations, or both — gets reproduced over `C`.

The third pattern is the same construction read once more, this time with `B` a localization. If `B = A[1/f]`, then

```text
B ⊗_A C = C[1/f].
```

Tensoring with a localization inverts the same element in `C`.

Three presentations, one universal property. When you see a tensor product, ask which of the three shapes one factor has
— polynomial ring, quotient, or localization — and apply the matching rule.

## Base change: changing the parameter space

The tensor product, once defined, immediately names a geometric operation we have not yet had words for: changing the
parameter space underneath a family. The algebraic name is **base change**. Suppose `A → A'` is a ring map and `B` is an
`A`-algebra. Then

```text
B' := B ⊗_A A'
```

is an `A'`-algebra, with structure map `A' → B'` given by `a' ↦ 1 ⊗ a'`. This `B'` is called the **base change** of `B`
along `A → A'`.

Geometrically: think of `Spec A` as the "parameter space" of a family of geometric objects `Spec B → Spec A`. A morphism
`Spec A' → Spec A` provides a "new parameter space." Base change pulls the family back to the new parameter space.

Two cases of base change matter most.

**Localizing the family.** Take `A → A_𝔭`. Then `B ⊗_A A_𝔭` is "the family `Spec B → Spec A`, viewed near the point
`𝔭`."

**Passing to the fiber.** Take `A → κ(𝔭) = A_𝔭/𝔭A_𝔭`. Then `B ⊗_A κ(𝔭)` is "the **fiber** of the family over the point
`𝔭`."

Both operations are tensor products. The notation `κ(𝔭)` for the residue field `Frac(A/𝔭)` of a prime `𝔭 ⊂ A` will be
standard from now on.

## Fibers, computed

Base change told us that the fiber over a prime is a tensor product with a residue field. The running example was
waiting for exactly this tool. We turn it on now.

Recall the example: `B = ℤ[t]/(t² − 2)` over `A = ℤ`. The fiber of the geometric morphism `Spec B → Spec ℤ` over a prime
`(p) ⊂ ℤ` is computed by base change:

```text
B ⊗_ℤ 𝔽_p = ℤ[t]/(t² − 2) ⊗_ℤ 𝔽_p = 𝔽_p[t]/(t² − 2),
```

using the "killing relations" pattern.

So the fiber over `(p)` is `Spec 𝔽_p[t]/(t² − 2)`, and its structure depends on how `t² − 2` factors over `𝔽_p`. Three
cases.

**`p = 7`.** The squares mod `7` are `{0, 1, 2, 4}` (compute: `3² = 9 ≡ 2 mod 7`). So `2` is a square mod `7`, and
`t² − 2 = (t − 3)(t + 3) mod 7`. The fiber is

```text
𝔽₇[t]/(t² − 2) = 𝔽₇[t]/(t − 3)(t + 3) ≃ 𝔽₇ × 𝔽₇
```

by the Chinese remainder theorem. Two distinct points, each with residue field `𝔽₇`. **Split.**

**`p = 5`.** The squares mod `5` are `{0, 1, 4}`. So `2` is not a square, `t² − 2` is irreducible, and the fiber is

```text
𝔽₅[t]/(t² − 2) ≃ 𝔽_25,
```

a finite field of `25` elements. One point, with residue field `𝔽_25`. **Extension.**

**`p = 2`.** Now `2 ≡ 0 mod 2`, so `t² − 2 ≡ t² mod 2`. The fiber is

```text
𝔽₂[t]/(t² − 2) = 𝔽₂[t]/(t²),
```

the "fat point" with `t² = 0`. One point, with residue field `𝔽₂`, and a nilpotent direction `t` sticking out.
**Ramified.**

The trichotomy:

```text
p = 7:  two distinct points        — split
p = 5:  one point, bigger field    — extension
p = 2:  one point, with nilpotent  — ramified
```

This three-way split is the central rhythm of arithmetic geometry, and the central thread of Exposé I. We will sharpen
it in file 04 into the trichotomy "étale (split or extension) versus ramified."

For now, hold the picture: a single morphism of schemes, three qualitatively different fiber behaviors at three
different primes.

## Noetherian: a finiteness condition

We close with the technical hypothesis that runs underneath all of Exposé I.

A ring `A` is **noetherian** if every ascending chain of ideals `I₁ ⊆ I₂ ⊆ …` is eventually constant. Equivalently:
every ideal of `A` is finitely generated.

Noetherianness rules out infinitely deep nesting of ideals. It is a "no pathologies" condition. Practically every ring
you meet in arithmetic geometry is noetherian: `ℤ`, fields, polynomial rings over fields, quotients and localizations of
these, completions of noetherian local rings. Hilbert's basis theorem says that finitely generated algebras over
noetherian rings are noetherian. The standard nonexample is the polynomial ring in infinitely many variables,
`k[t₁, t₂, t₃, …]`.

The source of Exposé I assumes "all preschemes are locally noetherian" from section I.2 onward. Concretely: every scheme
of interest has a covering by `Spec A`'s with `A` noetherian. This is invisible in practice; we mention it once and move
on.

## Where we have arrived

We have collected the algebra. The slogan that organized everything: a ring is the algebra of functions on a space, with
arrows reversed.

Here is the dictionary so far.

```text
algebra                          geometric meaning (cashed in file 02)
-----------------------          ------------------------------------
ring A                           the affine scheme Spec A
ring map A → B                   morphism Spec B → Spec A (arrow flipped)
ideal I ⊂ A                      a closed subscheme of Spec A
quotient A/I                     ring of functions on the closed subscheme
prime ideal 𝔭 ⊂ A                a point of Spec A
maximal ideal 𝔪 ⊂ A              a closed point of Spec A
residue field κ(𝔭) = Frac(A/𝔭)   the field of values at the point
nilpotent in A                   an infinitesimal direction
localization A[1/f]              open subscheme where f ≠ 0
localization A_𝔭                 local ring near the point 𝔭
tensor product B ⊗_A C           fibered product Spec B ×_{Spec A} Spec C
base change B ⊗_A A'             pulled-back family over Spec A'
B ⊗_A κ(𝔭)                       fiber over 𝔭
```

We started with the riddle that `ℝ[t]` has maximal ideals not visible to the geometry of `ℝ`. We end with a complete
algebraic toolkit. The running example `B = ℤ[t]/(t² − 2)` over `ℤ` showed the trichotomy split / extension / ramified,
which is the central rhythm of the rest of the project.

Geometry is next.


# Categories, rings, and locality

To talk about étale morphisms we need a notion of "space" that is fluent in algebra. The reading guide pointed at the
destination — an algebraic substitute for a covering map. The route there starts on the algebra side: rings, ideals,
quotients, prime ideals, residue fields, localization, tensor products. Some of this is review. All of it carries a
hidden geometric meaning that we will cash in starting in the next file.

One sentence organizes everything:

> A ring is the algebra of functions on a space.

Once you see that, the operations on rings — quotient, localization, tensor product — become operations on spaces.
Algebra and geometry stop being two subjects; they become two views of the same subject. This file sets up the algebra
side of that dictionary so that the geometry side, in file 02, takes care of itself.

A short note on intuition. The material that follows will move between two registers: algebra (rings, ideals, modules)
and the geometry that algebra encodes. We assume the reader is at home in the first register and is meeting the second
for the first time.

#### Aside (Rust + type-theory analogy).

_Skip this paragraph if Rust trait resolution and dependent-type- theory substitution are not part of your background;
nothing later depends on it._

For readers with those backgrounds: a ring matches a trait describing what operations a type supports — the bundle of
`Add`, `Mul`, `Zero`, `One`, with the usual axioms. Ring homomorphisms reverse the direction of geometric morphisms the
same way substitutions `Γ → Δ` in a dependent type theory run opposite to morphisms of contexts. Localization adjusts a
ring the way adding a typeclass instance for one specific value adjusts a type.

We start with a familiar object and look at it slowly.

## Polynomial rings as functions

Take the ring `ℝ[t]` of polynomials in one variable with real coefficients. You have seen this many times. We are going
to look at it carefully because it is the simplest place where the algebra-as-geometry slogan is visible.

A polynomial like `3t² − 5t + 7` does two things at once. As an algebraic expression, it is a string of coefficients
tagged with powers of `t`. As a function, it sends each real number `a ∈ ℝ` to the real number you get by plugging in
`a`:

```text
(3t² − 5t + 7)(2) = 12 − 10 + 7 = 9.
```

So every element of `ℝ[t]` defines a function `ℝ → ℝ`. Two polynomials that define the same function are equal as
polynomials (for `ℝ`, an infinite field, this is true; over finite fields it fails, and we will be careful when it
matters). The collection of all polynomials, with the obvious addition and multiplication of functions, is a ring. We
call this ring `ℝ[t]`.

The slogan in this case reads:

> `ℝ[t]` is the algebra of polynomial functions on the real line `ℝ`.

The same idea works in more variables. The ring `ℝ[x, y]` is the algebra of polynomial functions of two real variables,
defined on the plane `ℝ²`. The ring `ℝ[x, y, z]` is the algebra of polynomial functions on three-dimensional space `ℝ³`.
And so on. In each case, the ring is "what you can compute pointwise" — the operations of the ring are operations on
functions, computed pointwise.

A few other rings we will use, with their geometric reading.

```text
ℤ        — functions on the geometric object Spec ℤ
ℝ[t]     — polynomial functions on the real line
ℂ[t]     — polynomial functions on the complex line
ℝ[x, y]  — polynomial functions on the real plane
ℝ[x, y]/(x² + y² − 1)  — polynomial functions on the unit circle
ℤ/n      — functions on a finite scheme of n points
```

Some of these we have not earned yet. `Spec ℤ` and "scheme of `n` points" are vocabulary from later in this file and
from file 02. `/(x² + y² − 1)` involves taking a quotient, which we have not defined. We will get to all of it. For now,
just notice the pattern: every ring on this list pairs with a geometric object, and the operations of the ring are the
operations on functions on that object.

The discipline of algebraic geometry is to take this pairing seriously. Whatever ring you write down, **there is a
geometric object behind it**. We are going to build the language to make this precise.

## Notation: categories we use

This file assumes the categorical prerequisites listed in the reading guide: objects, morphisms, functors, natural
transformations, universal properties, limits and colimits, pullbacks. We will not redefine them. Four categories appear
throughout the file:

- **Set**: sets and functions.
- **Rng**: commutative rings with `1`, and ring homomorphisms.
- **A-Alg**: commutative `A`-algebras over a fixed ring `A`, and `A`-algebra homomorphisms.
- **A-Mod**: `A`-modules and `A`-linear maps.

The category of schemes, **Sch**, will appear in file 02.

## Ring homomorphisms: a refresher

The slogan pairs each ring with a space. Before we look at structure inside a single ring, we look at the maps that
connect rings to one another — and, by reversal, the maps that will connect their spaces.

A **ring homomorphism** `φ : A → B` is a function from one ring `A` to another ring `B` that respects the ring
operations. Three conditions:

```text
φ(a + a') = φ(a) + φ(a'),
φ(a · a') = φ(a) · φ(a'),
φ(1_A) = 1_B.
```

So `φ` carries sums to sums, products to products, and the unit of `A` to the unit of `B`. (Carrying `0` to `0` follows
automatically from the first condition.)

Three examples to recognize.

**Inclusion.** The inclusion `ℤ → ℝ` of integers into the reals is a ring homomorphism. So is `ℝ → ℂ`. Whenever a
smaller ring sits inside a larger one, the inclusion is a ring homomorphism.

**Evaluation.** Pick a real number `a`. The function

```text
ev_a : ℝ[t] → ℝ,    p(t) ↦ p(a)
```

that evaluates a polynomial at `a` is a ring homomorphism. The sum-of-polynomials evaluates to the sum of the values;
the product-of-polynomials evaluates to the product of the values; the constant polynomial `1` evaluates to `1`. So
`ev_a` respects all three structures.

There is one such homomorphism for every real number. Each "point of `ℝ`" gives a ring homomorphism `ℝ[t] → ℝ`. This is
a hint — the points of the geometric line correspond to ring homomorphisms into `ℝ`.

**Reduction.** Pick an integer `n > 0`. The function

```text
ℤ → ℤ/n,    a ↦ a mod n,
```

that reduces an integer modulo `n` is a ring homomorphism. The target `ℤ/n` is the ring of integers mod `n`, with
operations inherited from `ℤ`.

These three examples — inclusion, evaluation, reduction — cover most of the ring homomorphisms we will meet in this
file.

### A first hint of arrow-flipping

Look at evaluation again. `ev_a : ℝ[t] → ℝ` is a homomorphism **from the ring of functions on `ℝ`** **to the value field
`ℝ`**. Geometrically, what `ev_a` does is: pick the single point `a` of `ℝ`, extract the value of every polynomial
there.

So the ring homomorphism `ev_a : ℝ[t] → ℝ` corresponds, in geometric terms, to a single point `{a} → ℝ`. The ring map
runs from `ℝ[t]` to `ℝ`. The geometric map runs from `{a}` to `ℝ`. Same direction at the geometric level (point to
line), but the ring map runs the opposite way (functions on the line to functions on the point).

This direction-flipping is the central feature of the ring-as-functions dictionary. We will see it many more times. The
slogan that goes with it:

> A ring map `A → B` is the algebraic shadow of a geometric map running `Spec B → Spec A`, with the arrow flipped.

We have not defined `Spec` yet. Hold the slogan; we will return to it.

## Ideals: vanishing on a subset

Maps between rings gave us the first hint of arrow-flipping. They told us nothing yet about what lives inside a single
ring. The algebraic counterpart of a subset of the space turns out to be a particular kind of subset of the ring — the
**ideal**.

The motivation. Suppose we have a ring `A` of functions on some space `X` (think `A = ℝ[x, y]` and `X = ℝ²`). Suppose
`Z ⊆ X` is a subset of the space (think the `x`-axis `{y = 0}` inside `ℝ²`). A natural question: which functions on `X`
vanish identically on `Z`?

For our example: which polynomials in `ℝ[x, y]` vanish on the `x`-axis? The answer is: polynomials with no constant or
`x`-only term — equivalently, polynomials of the form `y · g(x, y)` for some polynomial `g`.

Three things to notice about this collection.

1. The zero polynomial vanishes on the `x`-axis. So the collection contains `0`.
1. If `f` and `f'` both vanish on the `x`-axis, then so does `f + f'`. The collection is closed under addition.
1. If `f` vanishes on the `x`-axis and `h` is **any** polynomial, then `h · f` also vanishes on the `x`-axis (anything
    times zero is zero). The collection is closed under multiplication by arbitrary elements of the ring.

The third property is the surprising one. The collection of "vanishing functions" is closed under multiplication not
just by itself, but by **any** function in the ring. Multiplication by an arbitrary ring element absorbs into the
collection.

This is the algebraic essence of "vanishing on a subset," and we abstract it into a definition.

### The definition

An **ideal** of a ring `A` is a subset `I ⊆ A` satisfying three conditions:

1. `0 ∈ I`.
1. If `a ∈ I` and `a' ∈ I`, then `a + a' ∈ I`.
1. If `a ∈ I` and `h ∈ A`, then `h · a ∈ I`.

An ideal is more than a sub-ring. A sub-ring is closed under sums and products with itself; an ideal is closed under
products with the **entire ambient ring**. That third condition is the whole point.

The geometric model behind this — vanishing on a subset — is the guide for everything we will do with ideals.

### Examples to anchor the definition

In `ℤ`, the multiples of `5`,

```text
(5) := { 5n : n ∈ ℤ } = { …, −10, −5, 0, 5, 10, … },
```

form an ideal. They contain `0`; the sum of two multiples of `5` is a multiple of `5`; any integer times a multiple of
`5` is a multiple of `5`. Geometrically, this is the ideal of "functions on `Spec ℤ`" vanishing at the prime `(5)`.

In `ℝ[x, y]`, the multiples of `y`,

```text
(y) := { y · g(x, y) : g ∈ ℝ[x, y] },
```

form an ideal. It is the ideal of polynomials vanishing on the `x`-axis.

In `ℝ[x, y]`, the multiples of `x² + y² − 1` form an ideal `(x² - y² − 1)`. It is the ideal of polynomials vanishing on
the unit circle (the locus where `x² + y² = 1`).

These three examples follow a pattern. We have a single element of the ring; the ideal is "all multiples of that
element." Such an ideal is called a **principal ideal**, and the notation `(f)` means "the principal ideal generated by
`f`" — that is, `{ h · f : h ∈ A }`.

### Generated by several elements

Sometimes one element is not enough. The ideal generated by elements `f₁, …, f_n ∈ A`,

```text
(f₁, …, f_n) := { h₁ f₁ + … + h_n f_n : h_i ∈ A },
```

is the set of all `A`-linear combinations of the `f_i`. It is the smallest ideal containing all of `f₁, …, f_n`.

Examples.

In `ℤ`, the ideal `(6, 10)` is the set of all integers of the form `6m + 10n`. By the Euclidean algorithm, every such
integer is a multiple of `gcd(6, 10) = 2`, and conversely every multiple of `2` is `6m + 10n` for some `m, n`. So
`(6, 10) = (2)`, the multiples of `2`. In `ℤ`, every ideal turns out to be principal — generated by a single element.

In `ℝ[x, y]`, the ideal `(x, y)` is the set of polynomials of the form `h₁ x + h₂ y`, equivalently the polynomials with
zero constant term. Geometrically, this is the ideal of polynomials vanishing at the origin `(0, 0)`.

In `ℝ[x, y]`, the ideal `(x − 1, y − 2)` is the polynomials vanishing at the point `(1, 2)`.

### One ideal worth flagging

In `ℝ[t]`, take the ideal `(t²) = { t² · g(t) : g ∈ ℝ[t] }`. This is the multiples of `t²`. Geometrically, what does it
cut out?

Naively, it should cut out "where `t² = 0`," which is just `t = 0`. So we might expect `(t²)` to do the same job as
`(t)`. But it does not. The two ideals are different: `t ∈ (t)` but `t ∉ (t²)`. The ideal `(t²)` is strictly smaller
than `(t)`.

Geometrically, `(t²)` is "the origin, with one extra infinitesimal direction." We will see this more carefully in the
section on quotient rings. For now, hold this distinction: ideals can distinguish "the same subset" with different
multiplicities, and that distinction will turn out to encode infinitesimal data.

## Quotient rings

Ideals named the functions that vanish on a subset, but they have not yet given us the ring of functions on the subset
itself. The quotient construction supplies it.

Given a ring `A` and an ideal `I ⊆ A`, the **quotient ring** `A/I` is built in two steps. First, declare two elements
`a, a' ∈ A` to be equivalent (`a ∼ a'`) if `a − a' ∈ I`. Second, take the set of equivalence classes, and inherit the
ring operations from `A`.

Concretely, every element of `A/I` is represented by some `a ∈ A`, and two representatives `a, a'` give the same element
of `A/I` iff their difference is in `I`. We write `[a]` or `a + I` for the equivalence class, or just `a` when context
makes clear we are working in the quotient.

The geometric reading. If `I` is the ideal of functions vanishing on a subset `Z ⊆ X`, then two functions on `X` are
equivalent modulo `I` iff their difference vanishes on `Z` iff they take the same values on `Z`. So `A/I` is "the ring
of functions on `Z`," seen as a ring in its own right.

> **`A/I` is the ring of functions on the subset cut out by `I`.**

The examples we have already met carry over.

**`ℤ/(5) = ℤ/5`.** The integers modulo `5`, with operations inherited from `ℤ`. A finite ring with five elements.
Geometrically, this is "functions on the closed point `(5) ∈ Spec ℤ`," and the five elements are the five possible
"values" a function on that point can take.

**`ℝ[x, y]/(y) = ℝ[x]`.** Setting `y = 0` reduces a polynomial in two variables to a polynomial in one variable.
Geometrically, this is the ring of polynomial functions on the `x`-axis, which is indeed `ℝ[x]`.

**`ℝ[x, y]/(x² + y² − 1)`.** The ring of polynomial functions on the unit circle. Two polynomials are equivalent iff
their difference vanishes on the circle.

### The infinitesimal twist

Now look at `ℝ[t]/(t²)`. Setting `t² = 0` does not collapse `ℝ[t]` all the way down to `ℝ`. Every element of the
quotient is represented by some polynomial in `ℝ[t]`, modulo polynomials divisible by `t²`. After reducing, every
element has the form

```text
a + bt    with    t² = 0,    a, b ∈ ℝ.
```

So `ℝ[t]/(t²)` is a two-dimensional `ℝ`-vector space, with basis `1, t`. Multiplication is
`(a + bt)(a' + b't) = aa' + (ab' + ba')t`, since `t · t = t² = 0`.

The element `t ∈ ℝ[t]/(t²)` is interesting. It is not zero in the quotient (because `t ∉ (t²)`). But it satisfies
`t² = 0`.

An element with this property — nonzero, but raised to a power equals zero — is called **nilpotent**. The ring
`ℝ[t]/(t²)` has a nonzero nilpotent.

What does this look like geometrically? The ideal `(t²)` cuts out "`t² = 0`," which set-theoretically is just `t = 0` —
the origin. But the **ring** `ℝ[t]/(t²)` has more structure than the ring of functions on a single point would have.
Functions on a single point should form a copy of `ℝ`, not of `ℝ ⊕ ℝ · t`.

The extra dimension is "an infinitesimal direction" at the origin. The nilpotent `t` measures "first-order perturbation
away from the origin." This ring will turn out to be the universal model of "a single point with one infinitesimal
direction sticking out," and we will return to it many times.

The take-away for now: an ideal `I` can have a richer ring `A/I` than its set-theoretic vanishing locus would suggest.
Ideals carry more information than just "where things are zero." That extra information is what makes algebraic geometry
richer than naive set theory.

## The kernel of a ring homomorphism

Ideals and ring maps were introduced separately. They are the same notion, seen from two sides. Given a ring
homomorphism `φ : A → B`, the **kernel** is

```text
ker(φ) := { a ∈ A : φ(a) = 0 } ⊆ A.
```

The kernel is always an ideal. It contains `0` (because `φ(0) = 0`); it is closed under addition
(`φ(a + a') = 0 + 0 = 0`); it is closed under multiplication by any `h ∈ A` (`φ(h · a) = φ(h) · 0 = 0`).

So every ring homomorphism `φ : A → B` produces an ideal `ker(φ) ⊆ A`. Conversely, every ideal `I ⊆ A` is the kernel of
the quotient map `A → A/I`. Ideals and "kernels of ring homomorphisms" are the same notion.

The example to remember. The evaluation homomorphism `ev_a : ℝ[t] → ℝ` has kernel "polynomials vanishing at `a`." A
polynomial `p(t)` vanishes at `a` iff `p(t)` is divisible by `t − a` (this is the factor theorem from elementary
algebra). So

```text
ker(ev_a) = (t − a),
```

the principal ideal generated by `t − a`. The first nontrivial example of "ideal = kernel of a ring map."

This identification is the algebraic-geometry version of the factor theorem: vanishing at a point is captured by the
ideal of multiples of `t − a`.

## Prime ideals

Every ideal cuts out something, but not everything an ideal cuts out deserves to be called a point. Two distinguished
classes — **prime** and **maximal** — are the ones that will. We meet primes first; the maximal case will fall out as a
strengthening.

A **prime ideal** of a ring `A` is an ideal `𝔭 ⊊ A` (proper, not the whole ring) satisfying:

> If `a · b ∈ 𝔭`, then `a ∈ 𝔭` or `b ∈ 𝔭`.

Equivalently: the complement `A \ 𝔭` is closed under multiplication. A product of two things outside `𝔭` is again
outside `𝔭`.

The notation `𝔭` is a fraktur "p," typeset in fraktur in printed mathematics texts; we use the Unicode glyph. There is
nothing special about the typography. It is just a letter that flags "this is a prime."

Why "prime"? Because in `ℤ`, the prime ideals are exactly the ones generated by prime numbers (plus the zero ideal). The
definition "if a product is in `𝔭` then one factor is" is the definition of "prime" for a number `p`: if `p | ab` then
`p | a` or `p | b`. The ideal-theoretic version generalizes the number-theoretic one.

### Examples

**`ℤ`.** The prime ideals are `(0)` and `(p)` for each prime number `p`. The ideal `(0)` is prime because if `ab = 0` in
`ℤ` then `a = 0` or `b = 0` (`ℤ` has no zero divisors). The ideal `(p)` is prime because of the elementary primality
property.

What about `(6)`? It is not prime. We have `2 · 3 = 6 ∈ (6)`, but neither `2 ∈ (6)` nor `3 ∈ (6)`. So `(6)` fails the
prime condition. (In fact `(6) = (2) ∩ (3)`, an intersection of two primes.)

**`ℂ[t]`.** The prime ideals are `(0)` and `(t − a)` for each `a ∈ ℂ`. `(0)` is prime because `ℂ[t]` has no zero
divisors. Each `(t − a)` is prime because if a product `pq` is divisible by `t − a`, then evaluation at `a` gives
`p(a) q(a) = 0`, so `p(a) = 0` or `q(a) = 0`, forcing `t − a` to divide `p` or `q`.

What about `(t² − 1) = ((t − 1)(t + 1))`? It is not prime: the product `(t − 1)(t + 1) ∈ (t² − 1)`, but neither
`(t − 1) ∈ (t² − 1)` nor `(t + 1) ∈ (t² − 1)` (each factor has degree 1, while elements of `(t² − 1)` have degree at
least 2 or are zero).

**`ℝ[t]`.** The prime ideals are `(0)`, `(t − a)` for each `a ∈ ℝ`, and `(F)` for each monic irreducible quadratic
`F = t² + bt + c` with `b² − 4c < 0`. The first two should be familiar; the third is new. A monic quadratic with
negative discriminant is irreducible over `ℝ`, and the corresponding ideal is prime by the same argument: if a product
`pq` is divisible by `F`, then irreducibility of `F` forces it to divide `p` or `q`.

### Why the zero ideal is interesting

For an integral domain `A` (a ring with no zero divisors), the ideal `(0)` is prime. This is just unpacking the
definition: if `ab ∈ (0)` then `ab = 0`, so `a = 0` or `b = 0`, so `a ∈ (0)` or `b ∈ (0)`.

Geometrically, the zero ideal corresponds to a special "point" of the geometric object behind `A`. We will call it the
**generic point**. For `Spec ℤ`, the generic point is `(0)`. For `Spec ℂ[t]`, the generic point is `(0)`.

The generic point is "every point at once," in a precise sense: its closure is the whole space. We will see exactly what
this means in the next file.

## Maximal ideals

Primes generalized "if it divides a product, it divides a factor" and gave us a candidate notion of point. The
strengthening promised above — the one that will give us _closed_ points and a residue field — is the maximal ideal.

A **maximal ideal** of a ring `A` is a proper ideal `𝔪 ⊊ A` such that no other proper ideal strictly contains `𝔪`.
Equivalently: if `I` is an ideal with `𝔪 ⊆ I ⊆ A`, then either `I = 𝔪` or `I = A`.

The notation `𝔪` is a fraktur "m." Same as for `𝔭`, just a letter flagging "this is a maximal ideal."

### Equivalent definition: A/𝔪 is a field

Here is the cleanest equivalent characterization.

> An ideal `𝔪 ⊊ A` is maximal iff the quotient ring `A/𝔪` is a **field**.

The proof is short. If `A/𝔪` is a field, every nonzero element of `A/𝔪` is invertible. So if `I` is an ideal strictly
containing `𝔪`, the image of any element of `I \ 𝔪` is a nonzero element of `A/𝔪`, hence invertible. That forces
`I/𝔪 = A/𝔪`, hence `I = A`. Conversely, if no proper ideal strictly contains `𝔪`, take any `a ∈ A \ 𝔪`. The ideal
`(a) + 𝔪` strictly contains `𝔪`, so it must be all of `A`. So `1 = h a + m` for some `h ∈ A` and `m ∈ 𝔪`. Reducing mod
`𝔪` gives `1 = [h] [a]` in `A/𝔪`, so `[a]` is invertible.

The take-away: **maximal ideals correspond to surjections onto fields.** Each maximal ideal `𝔪 ⊂ A` gives a quotient
field `A/𝔪`, and conversely each surjection `A ↠ k` onto a field has a maximal ideal as kernel.

We will call `A/𝔪` the **residue field** at the maximal ideal `𝔪`. The name comes from the geometric reading: think of a
maximal ideal as a "point" of the space behind `A`, and the residue field as "where values at that point live."

### Maximal implies prime

Every maximal ideal is prime. The proof: if `𝔪` is maximal then `A/𝔪` is a field; fields have no zero divisors; so if
`ab = 0` in `A/𝔪` then `a = 0` or `b = 0`, which is exactly the prime condition for `𝔪`.

The converse fails. The ideal `(0) ⊂ ℤ` is prime (`ℤ` is an integral domain) but not maximal (`ℤ/(0) = ℤ`, which is not
a field). So the prime ideals of `ℤ` strictly contain the maximal ones: every `(p)` is both prime and maximal, but `(0)`
is prime without being maximal.

This distinction — primes that are not maximal — is exactly the "generic point" phenomenon. We come back to it.

### Examples

**`ℤ`.** The maximal ideals are `(p)` for each prime number `p`. The residue field at `(p)` is `ℤ/(p) = 𝔽_p`, the finite
field with `p` elements. So each prime number gives a maximal ideal whose residue field is the finite field with that
prime number of elements.

**`ℂ[t]`.** The maximal ideals are `(t − a)` for each `a ∈ ℂ`. The residue field at `(t − a)` is `ℂ[t]/(t − a) = ℂ`. The
isomorphism `ℂ[t]/(t − a) ≃ ℂ` sends a polynomial to its value at `a` — exactly the evaluation homomorphism `ev_a`.

So in `ℂ[t]`, maximal ideals correspond bijectively to points of `ℂ`. Each maximal ideal `(t − a)` pairs with the point
`a`, and the residue field is `ℂ`. The set of maximal ideals **is** `ℂ`, in disguise.

This is the simplest case where "maximal ideals are points" works literally. Algebra reconstructs the geometry of the
line.

## The riddle of `ℝ[t]`

We can now state the puzzle that motivates the whole construction of algebraic geometry.

In `ℂ[t]`, maximal ideals correspond to points of `ℂ`. Clean.

In `ℝ[t]`, the maximal ideals are of two kinds.

**Real points.** For each `a ∈ ℝ`, the ideal `(t − a)` is maximal, with residue field `ℝ[t]/(t − a) = ℝ`. Each real
number gives a maximal ideal whose residue field is `ℝ`.

**Conjugate-pair points.** For each monic irreducible quadratic `F = t² + bt + c` with `b² − 4c < 0`, the ideal `(F)` is
maximal, with residue field `ℝ[t]/(F) ≃ ℂ`. The isomorphism sends `t` to one of the two conjugate complex roots of `F` —
but symmetrically: there is no algebraic preference between the two roots, so the ideal `(F)` is most naturally
identified with the **pair** of conjugate roots `±i√(c − b²/4) − b/2`.

So the maximal ideals of `ℝ[t]` are **the real numbers** plus **the conjugate pairs of non-real complex numbers**.
Together they cover all the algebraic data the ring `ℝ[t]` "knows about."

The geometry of `ℝ`, on the other hand, only sees the real numbers. The maximal ideals corresponding to conjugate
complex pairs have no counterpart in the real line.

> **The algebra of `ℝ[t]` sees points that the geometry of `ℝ` does not.**

If we want geometry to track the algebra faithfully, we need a notion of "space" that includes the missing conjugate
pairs as honest points. That is what algebraic geometry provides.

## What `Spec` will do

We will define, in the next file, a geometric object `Spec A` attached to every ring `A`. Its **points** will be the
**prime ideals** of `A`. For `ℝ[t]`, the points of `Spec ℝ[t]` will be:

- the real numbers (one point per maximal ideal `(t − a)`),
- the conjugate-pair "points" (one per maximal ideal `(F)` for irreducible quadratic `F` with negative discriminant),
- the generic point `(0)`.

The first two are closed points; the third is the generic point. For any ring, the prime ideals — not just the maximal
ones — make up the points of `Spec A`. The maximal ones are closed points; the others are generic points of various
closed subschemes.

Why include the non-maximal primes? Because they are forced. A ring homomorphism does not, in general, pull back maximal
ideals to maximal ideals; it pulls back primes to primes. So if we want `Spec A` to be functorial in `A`, we have to use
all primes.

That is the construction the next file does. For the rest of this file, we collect the remaining algebra we will need:
modules, localization, tensor products, base change, fibers.

## A second example: `ℤ[t]/(t² − 2)` and its primes

Meet the running example. The ring `B = ℤ[t]/(t² − 2)` will reappear under every lens we build: as a fiber-by-fiber
description in this file, geometrically as `Spec B → Spec ℤ` in file 02, in calculus as the support of `Ω¹` in file 03,
in the étale hierarchy in file 04, and in normalization and Galois theory in file 06. Each return shows the same object
refracted through whatever new structure we have just defined.

As an abelian group, `B` is free on the basis `1, t`, where `t` satisfies `t² = 2`. So every element of `B` looks like
`a + bt` with `a, b ∈ ℤ`. This is "the integers, plus a square root of `2`." It is the ring of integers in the number
field `ℚ(√2)`.

The map `ℤ → B` is the obvious inclusion. Geometrically, it gives a morphism `Spec B → Spec ℤ` (with the arrow flipped,
as always).

We are not going to compute the prime ideals of `B` from scratch. Instead, we will compute the **fibers** of
`Spec B → Spec ℤ` over each prime of `ℤ` once we have the tensor product set up. That is in a few sections. For now,
just register the example: a ring extension `ℤ → ℤ[t]/(t² − 2)`, with the geometric morphism running the other way.

## Where we are, halfway through

We have defined: rings, ring homomorphisms, ideals, quotient rings, kernels, prime ideals, maximal ideals, residue
fields. We have noticed that maximal ideals look like "points" and that the algebra often knows about more points than
the obvious geometry does.

The remaining algebra in this file: modules, localization (zoom into a point), local rings, tensor products (the
algebraic shadow of geometric pullback), base change, and fibers.

After that, the geometry — the actual construction of `Spec A` and the dictionary it builds — is in file 02.

## Modules: vector spaces over a ring

So far, "things over `A`" has meant other rings — `A`-algebras. Many of the constructions ahead want a looser notion:
data carrying an `A`-action, without any multiplication of its own. That notion is the **module**.

An `A`-**module** is an abelian group `M` with a multiplication `A × M → M`, `(a, m) ↦ am`, satisfying:

```text
1 · m = m,
(a + a') m = am + a'm,
a (m + m') = am + am',
(aa') m = a (a' m).
```

A module is a vector space with the field of scalars replaced by a ring. The flexibility lies in what `A` is.

- When `A = k` is a field, an `A`-module is exactly a `k`-vector space.
- When `A = ℤ`, an `A`-module is exactly an abelian group. (The scaling `n · m` is just `m + m + … + m` repeated `n`
    times.)
- When `A = k[t]`, an `A`-module is a `k`-vector space `V` together with a `k`-linear endomorphism `T : V → V`. (The
    scalar `t` acts by `T`; the polynomial `p(t)` acts by `p(T)`.)
- For any ring `A`, the ring `A` is itself an `A`-module (the scaling is just multiplication in `A`).

Three familiar things, all instances of one notion. Modules unify "vector space," "abelian group," and "vector space
with a chosen endomorphism" into a single language.

For us, the most important fact about modules is **finite generation**. An `A`-module `M` is **finitely generated** if
there exist `m₁, …, m_n ∈ M` such that every element of `M` is some `A`-linear combination of them. Equivalently, there
is a surjection `Aⁿ ↠ M`.

Two further adjectives for modules.

A module is **free** of **rank `n`** if `M ≃ Aⁿ`. A free module is a "vector space" over `A` in the most literal sense:
it has a basis.

A module is **flat** if tensoring with it (next section) preserves injections. Flatness is a "good behavior" condition;
we make it precise after we have tensor products. We use it in file 04.

The role of modules in our story. Whenever we have a ring map `A → B`, we can view `B` as an `A`-module (with `A` acting
via the map). Properties like "`B` is finite over `A`" or "`B` is flat over `A`" are then properties of `B` as an
`A`-module, and they control how the geometric morphism `Spec B → Spec A` behaves.

## Localization: zoom in by inverting

Modules let us carry data over a fixed ring. The next operation changes the ring itself, in a way that pictures cleanly
as restriction to an open subset of the underlying space. It is the most overtly geometric of the algebraic operations
in this file.

The motivation. Take `ℝ[x, y]`, the polynomial functions on `ℝ²`. Consider the function `x ∈ ℝ[x, y]`. It vanishes on
the `y`-axis and is nonzero everywhere else. On the open set where `x ≠ 0`, we can divide by `x`. The function `1/x` is
not in `ℝ[x, y]`, but on the open subset `{x ≠ 0}` it makes perfect sense.

What is the right ring of functions on the open subset where `x ≠ 0`? It should be `ℝ[x, y, 1/x]` — polynomial functions
of `x, y, 1/x`, where we have allowed `1/x`.

This is the operation we call **localization**. It builds the smallest ring containing `A` in which a chosen element is
invertible.

### The construction

For a ring `A` and an element `f ∈ A`, the **localization of `A` at `f`** is the set of formal fractions

```text
A[1/f] := { a / fⁿ : a ∈ A, n ≥ 0 } / ∼,
```

with two fractions `a/fⁿ` and `a'/fᵐ` declared equivalent iff there exists `k ≥ 0` such that `fᵏ (fᵐ a − fⁿ a') = 0` in
`A`. The ring operations are the obvious "common denominator" ones.

The natural map `A → A[1/f]` sends `a ↦ a/1`. It is a ring homomorphism, and its image is in the largest piece of
`A[1/f]` that does not need any division by `f`.

The map is universal in the following sense: any ring homomorphism `A → R` that sends `f` to a unit factors uniquely
through `A → A[1/f]`. So `A[1/f]` is "the smallest extension of `A` in which `f` is invertible."

### The geometric reading

We will see in the next file that

> **`A[1/f]` is the ring of functions on the open subset of `Spec A` where `f` does not vanish.**

In other words: localizing the algebra at `f` is the algebraic shadow of restricting `Spec` to the open subset where
`f ≠ 0`.

Three concrete cases to feel.

`ℤ[1/2]` is the ring of "rationals with denominator a power of `2`." Geometrically, it is the ring of functions on the
open subset of `Spec ℤ` complementary to the closed point `(2)`. We have removed the prime `2` from consideration.

`k[t][1/(t − a)]` is the ring of polynomials in `t` with denominators allowed to be powers of `t − a`. Geometrically,
the ring of functions on the open subset of `Spec k[t]` where `t − a ≠ 0`, that is, the affine line minus the point `a`.

`k[x, y][1/x]` is the ring of polynomials in `x, y` with denominators allowed to be powers of `x`. Geometrically, the
ring of functions on the open subset `{x ≠ 0}` of the affine plane — the plane minus the `y`-axis.

### Localization at a multiplicative set

The same construction works for a whole **multiplicatively closed subset** `S ⊆ A` (containing `1` and closed under
products). Define `S⁻¹A` as fractions `a/s` with `a ∈ A`, `s ∈ S`, with the analogous identification. Localization at a
single `f` is the special case `S = {1, f, f², …}`.

Two cases of `S` matter.

`S = {1, f, f², …}` for a single `f ∈ A`: gives `A[1/f]`.

`S = A \ 𝔭` for a prime ideal `𝔭 ⊂ A`: gives **`A_𝔭`**, the **localization of `A` at `𝔭`**. We invert everything not in
`𝔭`.

The second case is the one we use most. It deserves its own section.

## Localization at a prime: zoom in on a point

Take a prime `𝔭 ⊂ A`. The set `S = A \ 𝔭` is multiplicatively closed (because the complement of a prime ideal is closed
under multiplication). The localization `A_𝔭 := S⁻¹A` inverts every element not in `𝔭`.

What does this do? Algebraically: we adjoin inverses to every `s ∈ A` whose image in the residue field `A/𝔭` is nonzero.
We are "throwing in inverses for everything visible at `𝔭`."

The result is striking. **`A_𝔭` is a local ring with exactly one maximal ideal `𝔭 A_𝔭`.** The residue field at this
maximal ideal is `Frac(A/𝔭)`.

The geometric reading:

> **`A_𝔭` is the ring of functions defined on _some_ open neighborhood of `𝔭`, with two functions identified if they
> agree on a smaller neighborhood — equivalently, the stalk of the structure sheaf at `𝔭`.**

Localizing at `𝔭` is "zooming in on the point `𝔭`." Anything that was nonzero at `𝔭` becomes literally invertible.
Anything that was zero at `𝔭` is still in the maximal ideal of the localized ring.

### Examples

For `A = ℤ` and `𝔭 = (5)`:

```text
ℤ_(5) = { m/n ∈ ℚ : 5 ∤ n } = "rationals with denominator coprime to 5".
```

The maximal ideal is `(5) ⊂ ℤ_(5)` (the multiples of `5`). The residue field is `ℤ_(5) / (5) = ℤ/5 = 𝔽₅`.

For `A = ℤ` and `𝔭 = (0)`:

```text
ℤ_(0) = { m/n ∈ ℚ : n ≠ 0 } = ℚ.
```

The localization at the generic point gives back the field of fractions. (The notation `Frac(A)` is the field of
fractions of an integral domain `A`.) Localizing at `(0)` always gives the field of fractions, when `A` is a domain.

## Local rings: the algebra near a point

A **local ring** is a ring with exactly one maximal ideal. The unique maximal ideal we usually call `𝔪`; the residue
field `A/𝔪` we call `κ`.

Most local rings of interest are localizations `A_𝔭`. Other natural local rings include:

- Any field. The maximal ideal is `(0)` and the residue field is the field itself.
- The formal power series ring `k[[t]]` over a field `k`. The maximal ideal is `(t)`, the residue field is `k`.
- The `p`-adic integers `ℤ_p`. The maximal ideal is `(p)`, the residue field is `𝔽_p`.

The slogan to hold:

> A local ring `(𝒪, 𝔪)` is the algebra of an "infinitesimal neighborhood" of one point. The maximal ideal `𝔪` is the
> ideal of functions vanishing at that point. The residue field `κ = 𝒪/𝔪` is where the value at the point lives.

### Local homomorphisms

A **local homomorphism** between local rings is a ring map `(𝒪, 𝔪) → (𝒪', 𝔪')` sending `𝔪` into `𝔪'`. Equivalently, the
preimage of `𝔪'` is `𝔪`. Equivalently, the induced map of residue fields `κ → κ'` is well-defined.

The induced map of residue fields is called the **residue extension**. We will care about whether it is finite,
separable, trivial, and so on.

A great deal of Exposé I is about local homomorphisms. The art is to extract information about the global geometry from
a uniform statement about local homomorphisms.

## Tensor product: combining algebras over a base

Quotient and localization each modified one ring. We still need a way to combine two rings sitting over a common base —
the algebraic shadow of the geometric pullback we will eventually want. That combining operation is the **tensor
product**. We define it patiently because it is the place students most often lose footing.

The setup. Fix a ring `A`. We have two `A`-modules `M` and `N`, and we want a "product" of them over `A`. The cartesian
product `M × N` is **not** the right answer. The cartesian product represents pairs, but most natural constructions on
modules are **bilinear**, not pair-shaped.

A **bilinear** map `M × N → P` is a function that is `A`-linear in each argument separately:

```text
b(am + a'm', n) = a · b(m, n) + a' · b(m', n),
b(m, an + a'n') = a · b(m, n) + a' · b(m, n').
```

Bilinear maps are everywhere. Multiplication on a ring is bilinear. Dot product on a vector space is bilinear. The
pairing `M × Hom(M, P) → P` is bilinear. We want a single algebraic object that "represents" all bilinear maps out of
`M × N`.

That object is the tensor product.

### Definition

The **tensor product** of `M` and `N` over `A`, denoted `M ⊗_A N`, is an `A`-module equipped with a bilinear map

```text
M × N → M ⊗_A N,    (m, n) ↦ m ⊗ n,
```

with the **universal property**: for every bilinear map `b : M × N → P` (with `P` an `A`-module), there is a **unique**
`A`-linear map `b̃ : M ⊗_A N → P` such that `b(m, n) = b̃(m ⊗ n)` for all `m, n`.

Concretely, `M ⊗_A N` is generated by symbols `m ⊗ n` for `m ∈ M`, `n ∈ N`, modulo the bilinearity relations:

```text
(m + m') ⊗ n = m ⊗ n + m' ⊗ n,
m ⊗ (n + n') = m ⊗ n + m ⊗ n',
(am) ⊗ n = m ⊗ (an) = a · (m ⊗ n).
```

The third relation lets `A`-scalars "move freely across the `⊗` symbol." This is what "over `A`" means.

### Bilinearity in practice

The third relation is the load-bearing one. Watch it move a scalar in `M ⊗_ℤ N`:

```text
(2m) ⊗ n  =  2 · (m ⊗ n)  =  m ⊗ (2n).
```

The first equality is the third relation read left-to-right; the second is the same relation read right-to-left, this
time factoring the `2` through the right tensor argument. So `(2m) ⊗ n` and `m ⊗ (2n)` represent the _same_ element of
`M ⊗_ℤ N`.

This is what "scalars move freely across `⊗`" means concretely: a scalar attached to either factor can be moved to the
other, or pulled out front. The same calculation runs over any base ring `A`: a scalar `a ∈ A` can sit on the left of
`⊗`, on the right of `⊗`, or out front, and the three positions represent equal elements.

In practice we never compute inside a tensor product beyond moves like this. We use the universal property and a few
standard formulas.

### Tensor product of `A`-algebras

If `B` and `C` are `A`-algebras (rings with structure maps from `A`), the tensor product `B ⊗_A C` inherits a
multiplication:

```text
(b ⊗ c) · (b' ⊗ c') = (bb') ⊗ (cc').
```

This makes `B ⊗_A C` an `A`-algebra. There are two natural inclusions

```text
B → B ⊗_A C,    b ↦ b ⊗ 1,
C → B ⊗_A C,    c ↦ 1 ⊗ c.
```

Both compose with the structure maps from `A` to give the same map `A → B ⊗_A C`.

The **universal property as a coproduct**: for any `A`-algebra `R` and any pair of `A`-algebra maps `B → R`, `C → R`,
there is a unique `A`-algebra map `B ⊗_A C → R` factoring them. So `B ⊗_A C` is the **coproduct** of `B` and `C` in the
category of `A`-algebras.

This is the operation that, in the next file, will give the fibered product of schemes:

```text
Spec B ×_{Spec A} Spec C = Spec (B ⊗_A C).
```

Geometric pullback, on the algebra side, is the coproduct of algebras, which is the tensor product.

> **Memorize.** Geometric pullback corresponds to _coproduct_ of `A`-algebras — tensor product `B ⊗_A C` — not to the
> category-theoretic pullback of rings. (The categorical pullback in **Rng** is pairs of ring elements with matching
> images, a different operation.) The reason is the arrow-flipping: limits in the category of schemes become colimits in
> the category of rings. This is the place where students slip; we will revisit it in file 02.

### Three patterns to internalize

Almost every concrete tensor-product computation in this file is one of three patterns, each reading the same universal
property through a different presentation of `B`.

Adjoining variables is the first. When `B = A[t₁, …, t_n]` is a polynomial ring, tensoring with `C` carries the
variables along:

```text
B ⊗_A C = C[t₁, …, t_n].
```

The same operation, applied to a quotient, kills the same relations on the other side. If `B = A/I`, then

```text
B ⊗_A C = C / IC,
```

where `IC` is the ideal of `C` generated by the image of `I` under `A → C`. Adding generators on the `B` side and
imposing relations on the `B` side both transfer cleanly to `C`. The pattern is the universal property doing its job: a
map out of `B ⊗_A C` is exactly an `A`-algebra map out of `B` together with a compatible map out of `C`, so whatever
presentation `B` has — generators, relations, or both — gets reproduced over `C`.

The third pattern is the same construction read once more, this time with `B` a localization. If `B = A[1/f]`, then

```text
B ⊗_A C = C[1/f].
```

Tensoring with a localization inverts the same element in `C`.

Three presentations, one universal property. When you see a tensor product, ask which of the three shapes one factor has
— polynomial ring, quotient, or localization — and apply the matching rule.

## Base change: changing the parameter space

The tensor product, once defined, immediately names a geometric operation we have not yet had words for: changing the
parameter space underneath a family. The algebraic name is **base change**. Suppose `A → A'` is a ring map and `B` is an
`A`-algebra. Then

```text
B' := B ⊗_A A'
```

is an `A'`-algebra, with structure map `A' → B'` given by `a' ↦ 1 ⊗ a'`. This `B'` is called the **base change** of `B`
along `A → A'`.

Geometrically: think of `Spec A` as the "parameter space" of a family of geometric objects `Spec B → Spec A`. A morphism
`Spec A' → Spec A` provides a "new parameter space." Base change pulls the family back to the new parameter space.

Two cases of base change matter most.

**Localizing the family.** Take `A → A_𝔭`. Then `B ⊗_A A_𝔭` is "the family `Spec B → Spec A`, viewed near the point
`𝔭`."

**Passing to the fiber.** Take `A → κ(𝔭) = A_𝔭/𝔭A_𝔭`. Then `B ⊗_A κ(𝔭)` is "the **fiber** of the family over the point
`𝔭`."

Both operations are tensor products. The notation `κ(𝔭)` for the residue field `Frac(A/𝔭)` of a prime `𝔭 ⊂ A` will be
standard from now on.

## Fibers, computed

Base change told us that the fiber over a prime is a tensor product with a residue field. The running example was
waiting for exactly this tool. We turn it on now.

Recall the example: `B = ℤ[t]/(t² − 2)` over `A = ℤ`. The fiber of the geometric morphism `Spec B → Spec ℤ` over a prime
`(p) ⊂ ℤ` is computed by base change:

```text
B ⊗_ℤ 𝔽_p = ℤ[t]/(t² − 2) ⊗_ℤ 𝔽_p = 𝔽_p[t]/(t² − 2),
```

using the "killing relations" pattern.

So the fiber over `(p)` is `Spec 𝔽_p[t]/(t² − 2)`, and its structure depends on how `t² − 2` factors over `𝔽_p`. Three
cases.

**`p = 7`.** The squares mod `7` are `{0, 1, 2, 4}` (compute: `3² = 9 ≡ 2 mod 7`). So `2` is a square mod `7`, and
`t² − 2 = (t − 3)(t + 3) mod 7`. The fiber is

```text
𝔽₇[t]/(t² − 2) = 𝔽₇[t]/(t − 3)(t + 3) ≃ 𝔽₇ × 𝔽₇
```

by the Chinese remainder theorem. Two distinct points, each with residue field `𝔽₇`. **Split.**

**`p = 5`.** The squares mod `5` are `{0, 1, 4}`. So `2` is not a square, `t² − 2` is irreducible, and the fiber is

```text
𝔽₅[t]/(t² − 2) ≃ 𝔽_25,
```

a finite field of `25` elements. One point, with residue field `𝔽_25`. **Extension.**

**`p = 2`.** Now `2 ≡ 0 mod 2`, so `t² − 2 ≡ t² mod 2`. The fiber is

```text
𝔽₂[t]/(t² − 2) = 𝔽₂[t]/(t²),
```

the "fat point" with `t² = 0`. One point, with residue field `𝔽₂`, and a nilpotent direction `t` sticking out.
**Ramified.**

The trichotomy:

```text
p = 7:  two distinct points        — split
p = 5:  one point, bigger field    — extension
p = 2:  one point, with nilpotent  — ramified
```

This three-way split is the central rhythm of arithmetic geometry, and the central thread of Exposé I. We will sharpen
it in file 04 into the trichotomy "étale (split or extension) versus ramified."

For now, hold the picture: a single morphism of schemes, three qualitatively different fiber behaviors at three
different primes.

## Noetherian: a finiteness condition

We close with the technical hypothesis that runs underneath all of Exposé I.

A ring `A` is **noetherian** if every ascending chain of ideals `I₁ ⊆ I₂ ⊆ …` is eventually constant. Equivalently:
every ideal of `A` is finitely generated.

Noetherianness rules out infinitely deep nesting of ideals. It is a "no pathologies" condition. Practically every ring
you meet in arithmetic geometry is noetherian: `ℤ`, fields, polynomial rings over fields, quotients and localizations of
these, completions of noetherian local rings. Hilbert's basis theorem says that finitely generated algebras over
noetherian rings are noetherian. The standard nonexample is the polynomial ring in infinitely many variables,
`k[t₁, t₂, t₃, …]`.

The source of Exposé I assumes "all preschemes are locally noetherian" from section I.2 onward. Concretely: every scheme
of interest has a covering by `Spec A`'s with `A` noetherian. This is invisible in practice; we mention it once and move
on.

## Where we have arrived

We have collected the algebra. The slogan that organized everything: a ring is the algebra of functions on a space, with
arrows reversed.

Here is the dictionary so far.

```text
algebra                          geometric meaning (cashed in file 02)
-----------------------          ------------------------------------
ring A                           the affine scheme Spec A
ring map A → B                   morphism Spec B → Spec A (arrow flipped)
ideal I ⊂ A                      a closed subscheme of Spec A
quotient A/I                     ring of functions on the closed subscheme
prime ideal 𝔭 ⊂ A                a point of Spec A
maximal ideal 𝔪 ⊂ A              a closed point of Spec A
residue field κ(𝔭) = Frac(A/𝔭)   the field of values at the point
nilpotent in A                   an infinitesimal direction
localization A[1/f]              open subscheme where f ≠ 0
localization A_𝔭                 local ring near the point 𝔭
tensor product B ⊗_A C           fibered product Spec B ×_{Spec A} Spec C
base change B ⊗_A A'             pulled-back family over Spec A'
B ⊗_A κ(𝔭)                       fiber over 𝔭
```

We started with the riddle that `ℝ[t]` has maximal ideals not visible to the geometry of `ℝ`. We end with a complete
algebraic toolkit. The running example `B = ℤ[t]/(t² − 2)` over `ℤ` showed the trichotomy split / extension / ramified,
which is the central rhythm of the rest of the project.

Geometry is next.


# Appendix: black boxes

Exposé I and Exposé II quote several theorems without proof, citing later parts of SGA or EGA. The gentle reading treats
them the same way. This appendix collects every such black box in one place. For each, we record a one-sentence
statement, what the gentle reading uses it for, the files where it is cited, and the source.

Body files refer to a row by name in prose — e.g. "by Zariski's Main Theorem" — with the row's short slug given in the
first column for grep-ability.

The rows are ordered by topic. Reading top to bottom: definitional / categorical, then finite and quasi-finite
morphisms, then lifting, then flatness machinery, then local-ring properties, then topological invariance, then standard
commutative-algebra tools.

| Anchor                                            | Black box                                                            | Statement                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                | What we use it for                                                                                                                                                                                                                                                                                                                                                                                                 | Used in                    | Source                                                                                                                                                       |
| ------------------------------------------------- | -------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | -------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `formally-etale-lifting`                          | Lifting characterization of formal étaleness                         | A morphism `f : X → Y` is **formally étale** iff for every closed subscheme `Y₀′ ⊂ Y′` of an affine scheme defined by a nilpotent ideal of `𝒪_{Y′}`, every morphism `Y₀′ → X` over `Y` extends uniquely to a morphism `Y′ → X` over `Y`. The morphism `f` is **étale** iff it is formally étale and locally of finite presentation.                                                                                                                                                                                                                                                                                                                                                      | Equivalent modern definition of étale; the bridge between flat-and-unramified (gentle definition) and the lifting property is the conjunction of theorems I.5.5 and I.8.3.                                                                                                                                                                                                                                         | 04, 05, 11                 | EGA IV 17.1.1; EGA IV 17.3.1                                                                                                                                 |
| `quasi-finite-equivalences`                       | Quasi-finite module characterizations                                | For a local homomorphism `A → B` of noetherian local rings, the following are equivalent: (1) `B/𝔪B` is finite-dimensional over `k = A/𝔪`; (2) `𝔪B` is an ideal of definition of `B` and `κ(B)/k` is finite; (3) the completion `B̂` is finite as an `Â`-module.                                                                                                                                                                                                                                                                                                                                                                                                                          | Underwrites the three definitions of quasi-finite in file 04; (1) is taken as the primary definition, (2) and (3) follow.                                                                                                                                                                                                                                                                                          | 04                         | EGA 0 §0.7.4.2                                                                                                                                               |
| `zmt`                                             | Zariski's Main Theorem                                               | Let `f : X → Y` be quasi-finite at a point `x ∈ X`, with `Y` noetherian. There exists an open neighbourhood `U ∋ x` and a factorization `U → X′ → Y` with `U → X′` an open immersion and `X′ → Y` finite.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                | Identifies a net local algebra with `B_𝔫` for `B` finite over the base; backbone of the local-form theorem I.7.6.                                                                                                                                                                                                                                                                                                  | 05, 11, 12                 | EGA IV §18.12.1; cf. Chevalley's seminar                                                                                                                     |
| `chevalley-proper`                                | Chevalley's theorem on proper morphisms                              | A proper quasi-finite morphism is finite.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                | First proof of theorem I.5.1 (étale + radicial = open immersion): proper + flat + radicial + finite type collapses to rank-1 finite locally free.                                                                                                                                                                                                                                                                  | 05, 11                     | EGA III/IV; cf. EGA IV 8.11.1                                                                                                                                |
| `hensel`                                          | Hensel's lemma                                                       | **Algebra form.** For `A` a complete noetherian local ring with residue field `k`, every finite étale `k`-algebra `B̄` lifts uniquely to a finite étale `A`-algebra `B` with `B ⊗_A k ≃ B̄`. **Polynomial form (corollary).** A coprime factorization of a monic polynomial over `k` lifts to a factorization over `A`.                                                                                                                                                                                                                                                                                                                                                                    | Algebra form is the engine of theorem I.6.1 (étale extensions of complete local rings); polynomial form is the classical statement illustrated in file 05.                                                                                                                                                                                                                                                         | 05, 11                     | EGA IV §18.5.13 (algebra form); §17.7.3 (polynomial form)                                                                                                    |
| `flatness-sorites`                                | Flatness sorites                                                     | Composition, base change, and openness of flat morphisms of finite type; openness of the flat locus.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     | Closes proposition I.4.5 (étale locus is open) and underpins the sorites of proposition I.4.6.                                                                                                                                                                                                                                                                                                                     | 04, 05, 11, 12             | EGA IV                                                                                                                                                       |
| `local-flatness-criterion`                        | Local flatness criterion                                             | For `M` finitely generated over a noetherian local ring `(A, 𝔪)`, `M` is flat over `A` iff `M` is free over `A` iff `Tor₁^A(M, A/𝔪) = 0`.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                | Justifies "finite + flat over a local ring = free" used in the étale story; e.g. the rank-one collapse in the first proof of I.5.1 and the construction of finite locally free algebras.                                                                                                                                                                                                                           | 04, 05, 11                 | EGA 0 §0.10.1.3                                                                                                                                              |
| `completion-flat`                                 | Faithful flatness of `A → Â`                                         | The `𝔪`-adic completion of a noetherian local ring is faithfully flat over it: `A → Â` is flat, and `M ⊗_A Â = 0` implies `M = 0` for any `A`-module `M`.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                | Reduces local statements to the complete case and lets conclusions propagate back; e.g. the second proof of theorem I.5.1 and the lifting/equivalence story of section I.5.                                                                                                                                                                                                                                        | 05, 06, 11, 12             | EGA 0 §0.10.2.3, §0.7.6.16                                                                                                                                   |
| `completion-test-for-flatness`                    | Flatness lifts/descends along completion                             | For `M` finitely generated over a noetherian local ring `(A, 𝔪)`, `M` is flat over `A` iff its completion `M̂` is flat over `Â`.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          | Lets one check étaleness on the complete local rings; used implicitly in the second proof of I.5.1 ("reduce to `A` complete") and in the proof of I.6.1.                                                                                                                                                                                                                                                           | 05, 11                     | EGA 0 §0.10.2.3                                                                                                                                              |
| `dim-depth-transfer`                              | Dimension and depth under quasi-finite flat                          | A local quasi-finite flat homomorphism of noetherian local rings preserves Krull dimension and depth.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    | Opening of section I.9; routes the Cohen-Macaulay transfer and the Serre-criterion proof of I.9.5(i).                                                                                                                                                                                                                                                                                                              | 06, 12                     | EGA IV                                                                                                                                                       |
| `cm-transfer`                                     | Cohen-Macaulay transfer                                              | `A` is Cohen-Macaulay iff `B` is, for any local quasi-finite flat extension `A → B`.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     | Cited at the opening of section I.9 as one of the permanence properties already given by quasi-finite flatness.                                                                                                                                                                                                                                                                                                    | 06, 12                     | Corollary of `dim-depth-transfer`                                                                                                                            |
| `serre-normality`                                 | Serre's criterion for normality (R₁ + S₂)                            | For a noetherian ring `A`, `A` is normal iff every prime of height 1 gives a regular `A_𝔭` and every prime of height ≥ 2 has `depth A_𝔭 ≥ 2`.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            | First proof of theorem I.9.5(i): normality transfers along étale.                                                                                                                                                                                                                                                                                                                                                  | 06, 12                     | EGA IV 5.8.6                                                                                                                                                 |
| `nagata-finite`                                   | Nagata's theorem                                                     | The normalization of a noetherian Nagata ring `A` in a finite extension of `Frac(A)` is finite over `A`.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 | Section I.11 example (b): a complete integral local ring has finite normalization, hence its normalization is local.                                                                                                                                                                                                                                                                                               | 06, 12                     | Standard; cf. Nagata's _Local Rings_                                                                                                                         |
| `univ-homeo-equiv`                                | Universal-homeomorphism invariance of étale                          | If `Y′ → Y` is finite, radicial, and surjective (a universal homeomorphism), the functor `X ↦ X ×_Y Y′` is an equivalence on étale schemes. _Significance:_ étale-topological invariants are insensitive to inseparable phenomena like Frobenius.                                                                                                                                                                                                                                                                                                                                                                                                                                        | Section I.11 closing theorem; in particular Frobenius in characteristic `p` is invisible to the étale category.                                                                                                                                                                                                                                                                                                    | 06, 12                     | SGA 1 IX 4.10; EGA IV 18.10.3                                                                                                                                |
| `krull-intersection`                              | Krull's intersection theorem                                         | For a noetherian local ring `(A, 𝔪)` and a finitely generated `A`-module `M`, `⋂_n 𝔪^n M = 0`.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           | Step (ii) ⟹ (iii) of proposition I.3.1 (the differential criterion ⟹ diagonal criterion of unramifiedness).                                                                                                                                                                                                                                                                                                        | 03, 10                     | Standard                                                                                                                                                     |
| `nakayama`                                        | Nakayama's lemma                                                     | For `M` finitely generated over a local ring `(A, 𝔪)`, `M = 𝔪M` implies `M = 0`.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         | Used throughout to lift information from the residue field to the local ring. Cited explicitly in the proofs of I.3.1, I.7.4, I.7.5, I.7.10.                                                                                                                                                                                                                                                                       | 03–05, 10, 11, 12          | Standard                                                                                                                                                     |
| `hironaka-multiplicity`                           | Hironaka's multiplicity criterion                                    | For a dominant morphism `f : X → Y` of finite type of reduced preschemes, `y ∈ Y` with `𝒪_y` regular: if every component of `f⁻¹(y)` has multiplicity `1` and `f⁻¹(y)_red` is normal, then `f⁻¹(y)` is reduced (hence normal), `X` is normal at all points of `f⁻¹(y)`, and `f` is flat over `Y` at all points of `f⁻¹(y)`.                                                                                                                                                                                                                                                                                                                                                              | Used for the Hironaka smoothness criterion II.2.5–II.2.6 (file 14): converts multiplicity-1 fiber data into smoothness of `f`.                                                                                                                                                                                                                                                                                     | 14                         | EGA IV §5.12.10                                                                                                                                              |
| `nagata-open-regular`                             | Nagata's open-regularity theorem                                     | The set of regular points of a (sufficiently nice — e.g. excellent or Nagata) noetherian scheme is open.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 | Cited in II.5 (file 16) as the non-differential proof, due to Nagata, that breaks the circularity in the original differential-methods proof of II.5.5–II.5.6.                                                                                                                                                                                                                                                     | 16                         | Nagata, _Local Rings_; EGA IV §6.12                                                                                                                          |
| `mac-lane-separability`                           | Mac Lane's separability criterion via `Ω¹`                           | For a field extension `K / k`: `K` is separable over `k` iff `Ω¹_{K/k}` is generated by the differentials of a transcendence basis whose elements are linearly independent. Equivalently: every collection `f₁, …, f_n ∈ K` whose `df_i` are linearly independent in `Ω¹_{K/k}` is algebraically independent over `k`.                                                                                                                                                                                                                                                                                                                                                                   | Closes the proof of II.5.6 (file 16) without invoking Zariski's open-regularity theorem; the bridge between separability and the differential characterization of smooth field extensions.                                                                                                                                                                                                                         | 16                         | Bourbaki, _Algèbre_, Chapter V §16; Mac Lane, _Algebra_                                                                                                      |
| `serre-affine-vanishing`                          | Serre vanishing on affines                                           | For any affine scheme `X = Spec A` and any quasi-coherent `𝒪_X`-module `𝓕`, `H^i(X, 𝓕) = 0` for all `i ≥ 1`. The engine that makes Čech cohomology of quasi-coherent sheaves on a separated scheme computable on a single affine cover.                                                                                                                                                                                                                                                                                                                                                                                                                                                  | Throughout file 17; legitimizes every cocycle calculation in file 21 by ensuring the affine pieces of an affine cover are acyclic. Combined with Leray gives `Ȟ^p(𝔘, 𝓕) = H^p(X, 𝓕)`.                                                                                                                                                                                                                              | 17, 21, 22                 | Serre, _Faisceaux algébriques cohérents_ III.3; EGA III §1.3.1                                                                                               |
| `leray-acyclic-cover`                             | Leray's acyclic-cover theorem                                        | If `𝔘 = (U_i)` is an open cover of `X` such that every finite intersection `U_{i₀…i_p}` is `F`-acyclic (i.e. `H^q(U_{i₀…i_p}, F) = 0` for `q ≥ 1`), then the natural map `Ȟ^p(𝔘, F) → H^p(X, F)` is an isomorphism for every `p`.                                                                                                                                                                                                                                                                                                                                                                                                                                                        | Combined with `serre-affine-vanishing`, lets one compute sheaf cohomology of any quasi-coherent sheaf on a separated noetherian scheme from a single affine open cover. Used everywhere in file 17 and file 21.                                                                                                                                                                                                    | 17, 21                     | Godement, _Topologie algébrique et théorie des faisceaux_ II.5; EGA 0_III §12.4                                                                              |
| `formally-smooth-ega`                             | EGA lifting-property definition of formally smooth                   | EGA 0_IV §19.3.1 defines: a topological `A`-algebra `B` is **formally smooth** over `A` if for every discrete topological `A`-algebra `C` and every nilpotent ideal `J ⊂ C`, every continuous `A`-homomorphism `B → C/J` lifts to a continuous `A`-homomorphism `B → C`. Drops SGA III.1.1's finite-residual hypothesis.                                                                                                                                                                                                                                                                                                                                                                 | Cited from file 19 as the modern conceptual framing of III.1.1; the lifting property is what III.2.1 recovers from the formal-power-series definition. EGA's definition is the one used throughout EGA 0_IV §19 onwards.                                                                                                                                                                                           | 19                         | EGA 0_IV §19.3.1                                                                                                                                             |
| `cohen-ring`                                      | Cohen ring and structure theorem                                     | A **Cohen ring** is a complete noetherian local ring with maximal ideal `(p)` (or `(0)`) and perfect residue field. Cohen's structure theorem (EGA 0_IV §19.8.6): every complete noetherian local ring `(A, 𝔪, k)` is a quotient of `W[[T₁, …, T_n]]` for some Cohen ring `W` with the same residue field. For perfect `k` of char `p > 0`, `W = W(k)` is the ring of Witt vectors.                                                                                                                                                                                                                                                                                                      | Cited from file 22 as the foundation for the algebraization story over a complete local ring with residue field of mixed characteristic. The Cohen ring is the universal mixed-characteristic lift of the residue field.                                                                                                                                                                                           | 22                         | EGA 0_IV §19.8.6; Serre, *Local Fields* II §6 (Witt vectors)                                                                                                 |
| `coefficient-field`                               | Coefficient field theorem                                            | For an equicharacteristic complete local ring `(A, 𝔪, k)` (i.e. `char A = char k`), there exists a subfield `k' ⊂ A` with `A → k` inducing an isomorphism `k' ≃ k`. The "coefficient field." This realizes `A` as a quotient of `k[[T₁, …, T_n]]` (the equicharacteristic case of Cohen's structure theorem).                                                                                                                                                                                                                                                                                                                                                                            | Cited from file 22 as the foundation for the trivial lift `X = X₀ ⊗_k A` of any `k`-scheme in equicharacteristic; first obstruction to lifting in `H²(𝔤) ⊗ 𝔪/𝔪²` automatically vanishes when a coefficient field exists.                                                                                                                                                                                           | 22                         | EGA 0_IV §19.6                                                                                                                                               |
| `grothendieck-existence`                          | Grothendieck existence theorem                                       | EGA III §5.4.1: for `X → Spec R` proper with `R` a complete noetherian ring with ideal of definition `𝓘`, the functor `𝓕 ↦ 𝓕̂` from coherent sheaves on `X` to coherent sheaves on the formal completion `X̂` is an **equivalence of categories**. Algebraic and formal coherent sheaves agree.                                                                                                                                                                                                                                                                                                                                                                                            | Cited from files 18, 21, 22 as the algebraization theorem for coherent sheaves. Combined with `algebraization-via-ample`, upgrades formal smooth lifts to algebraic smooth lifts (Theorem III.7.3).                                                                                                                                                                                                                | 18, 21, 22                 | EGA III §5.4.1                                                                                                                                               |
| `algebraization-via-ample`                        | Algebraization criterion via ample sheaf                             | EGA III §4.7.1: a formal scheme `𝔛` proper over `Ŝ` (with `S` complete noetherian) equipped with a coherent formal sheaf `ℒ̂` whose restriction `ℒ₀ = ℒ̂\|_{X₀}` is an ample line bundle on the closed fiber is **algebraizable**: there exists a projective scheme `X / S` with `X̂ ≃ 𝔛` and an ample line bundle on `X` inducing `ℒ̂`.                                                                                                                                                                                                                                                                                                                                                     | Cited from file 22 as the algebraization criterion for formal schemes themselves. Combined with `grothendieck-existence` (algebraization of sheaves), upgrades III.6.10's formal smooth lift to III.7.3's algebraic smooth lift.                                                                                                                                                                                   | 22                         | EGA III §4.7.1                                                                                                                                               |
| `giraud-torsor`                                   | H¹ classifies torsors                                                | For a sheaf of (commutative) groups `𝒢` on a topological space (or scheme) `T`, the set of isomorphism classes of **torsors** (= principal homogeneous sheaves) under `𝒢` is in canonical bijection with `H¹(T, 𝒢)`. The trivial class corresponds to torsors admitting a global section.                                                                                                                                                                                                                                                                                                                                                                                                | Cited from file 17 as the classification theorem for torsors; underwrites the H¹ obstruction in §III.5 (file 21). The non-abelian generalization (Giraud, _Cohomologie non abélienne_) is what Grothendieck's grumpy footnote III.5.0 alludes to.                                                                                                                                                                  | 17, 21                     | Giraud, _Cohomologie non abélienne_ (Springer 1971); Milne, _Étale Cohomology_ III §4                                                                        |
| `tor-derived-functor`                             | Tor as left-derived tensor product                                   | For a commutative ring `A` and `A`-modules `M, N`, the Tor functor `Tor^A_i(M, N)` is the `i`-th left-derived functor of `M ⊗_A −`, computable from any free resolution of either argument. Independence of the resolution, symmetry `Tor^A_i(M, N) ≅ Tor^A_i(N, M)`, and the long exact sequence in either argument hold.                                                                                                                                                                                                                                                                                                                                                               | Cited from file 23 as the framework for the entire flatness story of files 24–27. The vanishing characterization of flatness (`M` flat ⟺ `Tor^A₁(M, N) = 0` for all `N`) is the bridge between the algebraic definition and the Tor calculus.                                                                                                                                                                      | 23, 24, 25, 26             | Cartan-Eilenberg, _Homological Algebra_ Ch. VI; Weibel, _Introduction to Homological Algebra_ §3.2–§3.3                                                      |
| `spectral-sequence-composite-functors`            | Grothendieck spectral sequence for composite functors                | For composable additive right-exact functors `F : 𝓐 → 𝓑` and `G : 𝓑 → 𝓒` between abelian categories with enough projectives, with `F` carrying projectives to `G`-acyclics: `E²_{p, q} = (L_p G)((L_q F)(M)) ⇒ L_{p+q}(G ∘ F)(M)`. The five-term exact sequence in low degrees is the form most often used.                                                                                                                                                                                                                                                                                                                                                                              | Cited from file 23 (where the construction is collected) and used in file 26's proof of IV.5.2 (Serre's lemma extending flatness through `A → B`). Cited exactly once in the Exposé IV rewrite.                                                                                                                                                                                                                    | 23, 26                     | Cartan-Eilenberg, _Homological Algebra_ XV.5; Weibel §5.8                                                                                                    |
| `noether-normalization`                           | Noether's normalization lemma                                        | Every finitely generated `K`-algebra `B` over a field `K` is finite over a polynomial subring `K[t₁, …, t_n]` for some algebraically independent `t₁, …, t_n ∈ B`. The integer `n` equals the Krull dimension of `B`. Geometrically: every affine variety of dimension `n` over a field admits a finite map to `𝔸^n`.                                                                                                                                                                                                                                                                                                                                                                    | Cited from file 23 (where the geometric reading is unpacked) and used in file 27's proof of IV.6.7 (the generic-freeness lemma). The inductive base of the generic-flatness story.                                                                                                                                                                                                                                 | 23, 27                     | Atiyah-Macdonald §5.1; Bourbaki AC V §3.1                                                                                                                    |
| `chevalley-constructibility`                      | Chevalley's constructibility theorem                                 | For `f : X → Y` of finite type with `Y` noetherian, the image `f(X)` is a constructible subset of `Y`. The fundamental constructibility theorem of algebraic geometry; the proof routes through Noether normalization and the dichotomy of IV.6.1.                                                                                                                                                                                                                                                                                                                                                                                                                                       | Cited from file 27 (used in IV.6.6's proof to license the application of IV.6.3 to `f(X)`). Without Chevalley, the open-image theorem reduces to a much weaker statement.                                                                                                                                                                                                                                          | 27                         | EGA IV §1.8.4; Stacks tag 054K                                                                                                                               |
| `bourbaki-flat-modules`                           | Bourbaki's exposition of flat modules                                | Bourbaki, _Algèbre Commutative_, Chap. I (Modules plats): the canonical reference for the sorites of flat modules, including the case of a non-commutative base ring (where some statements need adjustment). Develops flatness, faithful flatness, and base change in a categorically clean way.                                                                                                                                                                                                                                                                                                                                                                                        | Cited at the opening of file 24 as the canonical alternative source for §§1–2 of Exposé IV. SGA 1 IV explicitly cites it in its opening footnote IV.0.3.                                                                                                                                                                                                                                                           | 24                         | Bourbaki, _Algèbre Commutative_, Chap. I, Hermann 1961                                                                                                       |
| `ega-fibers-systematic`                           | EGA IV §§11–12: systematic study of fibers                           | EGA IV §§11–12 carries out the systematic study of fiber dimension, cohomological dimension, and depth for `X → Y` locally of finite type, equipped with a coherent sheaf `F` flat over `Y`. Includes Bertini- and Seidenberg-type theorems for hyperplane sections and the comparison of properties of `Y`, `F`, and the fibers.                                                                                                                                                                                                                                                                                                                                                        | Cited at the opening of file 24 as the long-form sequel to SGA 1 IV. The fiber-dimension content is referenced in passing in Grothendieck's grumpy footnote to IV.6.6 (file 27); the answer to the equidimensionality question lives there.                                                                                                                                                                        | 24, 27                     | EGA IV §11, §12                                                                                                                                              |
| `bourbaki-sem-195`                                | Strict pro-representability of left-exact functors                   | Let `𝒞` be a category in which every object is **artinian** (every descending chain of subobjects stabilises), and let `F : 𝒞 → Set` be a left-exact functor with finite values. Then `F` is **strictly pro-representable**: there exists a normalised pro-object `P = (P_i)_{i ∈ I}` (transition maps epimorphisms; every epi `P_i → P′` equivalent to one `φ_{ji}`) such that `F(X) ≅ colim_i Hom(P_i, X)`. The `P` is determined up to unique isomorphism.                                                                                                                                                                                                                            | Cited from file 29 (where the construction is collected) and used in file 31 step (c) of the proof of V.4.1 to build the pro-object `P` representing the fibre functor `F`. The output drives the construction of `π = Aut(F)`.                                                                                                                                                                                    | 29, 31                     | Grothendieck, _Technique de descente et théorèmes d'existence en géométrie algébrique_ II, Séminaire Bourbaki **195**, Feb 1960                              |
| `bourbaki-sem-190`                                | Faithfully flat descent (Grothendieck)                               | Faithful flat descent: a finite faithfully flat morphism is a strict epimorphism in the category of preschemes; properties such as "finite", "étale", and the existence of a quotient `X / G` for an admissible action are invariant under faithfully flat quasi-compact descent. **Now unpacked in files 40–43 (gentle Exposé VIII): VIII.1.1 (descent of quasi-coherent modules), VIII.2.1 (affine descent), VIII.5.2 (descent of morphisms in `Sch`), VIII.5.7 (descent of finite locally free), and VIII.7.5–8 (effectivity criteria) collectively cover the cited content.** The earlier files 30, 31, 33 still cite this entry by name; the substantive proofs are in files 40–43. | Originally cited from file 30 (V.2.6's equivalence (i) ⟺ (ii) ⟺ (iii) for principal coverings), file 31 (V.3.6's strict-epi statement for surjective finite étale morphisms), and file 33 (V.7's "principal covering ⟺ principal homogeneous in `𝒞(S)`"). Now also the conceptual umbrella for files 40–43; specific results are cross-referenced from there.                                                      | 30, 31, 33, 40, 41, 42, 43 | Grothendieck, _Technique de descente et théorèmes d'existence en géométrie algébrique_, Séminaire Bourbaki **190**, May 1959                                 |
| `ega-ii-affine-morphisms`                         | Affine morphisms as `Spec` of quasi-coherent algebras                | An `S`-prescheme `X → S` is **affine** (i.e. `f⁻¹(U)` is affine for every affine open `U ⊂ S`) iff `X = Spec_S(𝒜)` for a unique quasi-coherent `𝒪_S`-algebra `𝒜`. The functor `𝒜 ↦ Spec_S(𝒜)` is an anti-equivalence between quasi-coherent `𝒪_S`-algebras and affine `S`-preschemes. Tensor operations on the algebra side correspond to fibre products and base change on the prescheme side.                                                                                                                                                                                                                                                                                          | Cited from files 41 (§VIII.2 affine descent reduces to descent of quasi-coherent algebras) and 42 (§VIII.5.6: affine morphisms descend). Used implicitly throughout files 40–43 whenever a prescheme is replaced by its quasi-coherent algebra of functions.                                                                                                                                                       | 41, 42                     | EGA II §1.4                                                                                                                                                  |
| `ega-ii-proj-and-ample`                           | `Proj`-construction and ample invertible sheaves                     | For a quasi-coherent graded `𝒪_S`-algebra `𝒮 = ⊕_{n ≥ 0} 𝒮_n`, the **`Proj`-construction** `Proj_S(𝒮) → S` is a separated morphism whose fibers are the usual `Proj` of fibers. An invertible sheaf `𝓛` on `X` is **ample relative to `f : X → S`** iff for every `s ∈ S` there is an affine open neighbourhood `U ∋ s` such that `f⁻¹(U)` embeds as a closed subscheme into a projective `U`-space via a power `𝓛^{⊗n}\|_{f⁻¹(U)}`. Equivalently, `X` is `S`-isomorphic to an open of `Proj_S(⊕ f_*(𝓛^{⊗n}))`. Ampleness is preserved by base change.                                                                                                                                   | Cited from files 42 (§VIII.5.8 ampleness descends; §VIII.5.9 quasi-affineness via `𝓛 = 𝒪_X`) and 43 (§VIII.7.8 ample-invertible-sheaf effectivity criterion). The norm-of-an-ample-sheaf construction in §VIII.7.7 also leans on EGA II 6.6.4.                                                                                                                                                                     | 42, 43                     | EGA II §3, §4 (Proj construction and ample sheaves); §5.1 (quasi-affine via `𝒪_X` ample)                                                                     |
| `ega-ii-norm-finite-locally-free`                 | Norm of a section under a finite locally free morphism               | For a finite locally free morphism `f : X → S` of constant rank `r`, multiplication by a local section `s ∈ Γ(X, 𝒪_X)` defines an `𝒪_S`-linear endomorphism of `f_*(𝒪_X)` (locally free of rank `r`); its **determinant** is a section `N_f(s) ∈ Γ(S, 𝒪_S)`, the **norm** of `s`. Properties: (a) compatible with base change `Y → S`, hence `N_{f_Y}(s_Y) = N_f(s)\|_Y`; (b) `f^*(N_f(s))` divides `s^r` in `𝒪_X`; (c) the vanishing locus of `N_f(s)` is the image `f(Z(s))` of the vanishing locus of `s`. Consequently `S_{N_f(s)} = S − f(Z(s))`.                                                                                                                                   | Cited from file 43 (§VIII.7.6 finite locally free effectivity criterion; the norm of a section produces a saturated open of the upstairs scheme).                                                                                                                                                                                                                                                                  | 43                         | EGA II §6.5 (norm map for finite locally free morphisms); §6.6.4 (norm of an ample invertible sheaf)                                                         |
| `ega-iv-constructibility-via-finite-presentation` | Reductions of finite-presentation morphisms to noetherian situations | Every finite-presentation morphism `f : X → Y` of preschemes is obtained from a finite-type morphism `f₀ : X₀ → Y₀` between *noetherian* preschemes by base change along `Y → Y₀`. Concretely, when `Y = Spec A` is affine, one can find a finite-type subring `A₀ ⊂ A` and an `A₀`-scheme `X₀` of finite type with `X = X₀ ×_{A₀} A`. This is the standard "reduce to noetherian" device used to drop noetherian hypotheses in EGA-style proofs.                                                                                                                                                                                                                                        | Cited from file 42 (§VIII.6.6 remark on dropping noetherian hypotheses from VIII.6.1–6.2 for finite-presentation morphisms) and file 43 (§VIII.7.9 generalisation of the quasi-affine effectivity criterion to non-noetherian bases via finite presentation).                                                                                                                                                      | 42, 43                     | EGA IV §8 (passage à la limite for noetherian reductions); EGA IV §17–18 (étale and smooth versions)                                                         |
| `minkowski-discriminant`                          | Minkowski's discriminant inequality                                  | The discriminant of any non-trivial number field `K ≠ ℚ` has absolute value `> 1`. Equivalently: every non-trivial finite extension `K / ℚ` is ramified at some prime. Geometrically: `Spec ℤ` admits no non-trivial finite étale covering, hence `π₁^ét(Spec ℤ) = (1)`.                                                                                                                                                                                                                                                                                                                                                                                                                 | Cited from file 33 in the worked example `S = Spec ℤ` of the étale fundamental group. The "no unramified extensions" content is one of the most striking arithmetic facts attached to the étale `π₁`.                                                                                                                                                                                                              | 33                         | Minkowski, _Geometrie der Zahlen_ (1896); Neukirch, _Algebraic Number Theory_ III.2.18                                                                       |
| `lenstra-galois-theory-schemes`                   | Lenstra: Galois Theory for Schemes (lecture notes)                   | H. W. Lenstra, _Galois Theory for Schemes_, online lecture notes (Universiteit Leiden / Berkeley). Modern long-form alternative to SGA 1 V: Galois categories axiomatised in Ch. 3 (with a streamlined axiom system that drops the "strict" qualifier on epimorphisms), finite étale algebras in Ch. 4, finite étale morphisms in Ch. 5, complements (Spec of a field, infinite Galois extensions) in Ch. 6.                                                                                                                                                                                                                                                                             | Cited as the modern reader's-companion reference throughout files 28–33. Particular cross-references: Ch. 3 for the alternative axiomatic of file 31; Ch. 5 for the étale-cover side of files 30–31; Ch. 6 for the file 33 Spec(k) example.                                                                                                                                                                        | 28, 29, 30, 31, 32, 33     | Lenstra, _Galois Theory for Schemes_, lecture notes (Leiden / Berkeley)                                                                                      |
| `murre-cadoret-galois-categories`                 | Cadoret (after Murre): Galois Categories                             | A. Cadoret (after J. P. Murre), _Galois Categories_. Modern reference for Exposé V §§4–6: Ch. 2 develops the axiomatics; Ch. 3 carries out the proof of the main theorem (V.4.1) cleanly; Ch. 4 the functoriality (V.6); Ch. 5 the étale-cover side (V.3); Ch. 6 examples (Spec of a field, the first homotopy sequence, comparison theorems including the `π₁^{ét} = π̂₁^{top}` for proper varieties over `ℂ`).                                                                                                                                                                                                                                                                          | Cited as the cleanest modern walk-through of V.4.1 (file 31 leans on Ch. 3) and as the source of the worked examples in file 33. The first-homotopy-sequence material in Ch. 6 is the natural follow-up to file 33's Spec(𝔽_p) and elliptic-curve examples.                                                                                                                                                        | 28, 30, 31, 32, 33         | Cadoret, _Galois Categories_; Murre, _Lectures on an Introduction to Grothendieck's Theory of the Fundamental Group_ (Tata IFR, 1967)                        |
| `topological-covering-space-classification`       | Topological covering-space classification (Galois)                   | For a connected, locally arcwise connected, locally simply connected topological space `B` with base point `b₀ ∈ B`, the universal cover `B̃ → B` exists and the deck transformation group satisfies `Aut(B̃ / B) ≅ π₁^top(B, b₀)`. There is an equivalence between the category of connected coverings of `B` (up to iso) and the category of subgroups of `π₁^top(B, b₀)` (up to conjugacy). Restricting to _finite_ coverings gives an equivalence with open subgroups of `π̂₁^top(B, b₀)`.                                                                                                                                                                                              | Cited from file 28 as the topological prototype of the algebraic Galois theory. The finite-cover statement is the geometric template that file 31's Theorem V.4.1 generalises to schemes.                                                                                                                                                                                                                          | 28                         | Hatcher, _Algebraic Topology_, Ch. 1; May, _A Concise Course in Algebraic Topology_, Ch. 3                                                                   |
| `vistoli-fga-explained`                           | Vistoli: Notes on Grothendieck topologies (FGA Explained)            | A. Vistoli, _Notes on Grothendieck topologies, fibered categories and descent theory_, in _FGA Explained_ (Fantechi–Göttsche–Illusie–Kleiman–Nitsure–Vistoli), AMS 2005; arXiv math/0412512. Modern long-form treatment of SGA 1 VI + VIII + IX. Ch. 2 sites and sheaves; Ch. 3 fibered categories with full proofs of cleavage / pseudofunctor equivalences; Ch. 4 descent for quasi-coherent sheaves and stacks.                                                                                                                                                                                                                                                                       | Cited as the modern reader's-companion reference throughout files 34–38. Particular cross-references: Vistoli §2.3 for Grothendieck topologies (file 34 Module A); §3.1.3–§3.1.4 for the cleavage-pseudofunctor 2-equivalence (file 37 §VI.8); §3.2 for the QCoh fibred category (file 34 Module C example, file 38 closing pointer); Ch. 4 for the fpqc-descent statements deferred to a future Exposé VIII pass. | 34, 35, 36, 37, 38         | Vistoli, _Notes on Grothendieck topologies, fibered categories and descent theory_, arXiv math/0412512v4 (May 2007); FGA Explained, AMS 2005                 |
| `mac-lane-coherence`                              | Mac Lane's coherence theorem                                         | For a pseudofunctor (or, more generally, any monoidal-category-like or 2-categorical structure with associator and unitor data), the pentagon identity plus the unit conditions force *every* diagram of coherence isomorphisms to commute. In particular, every two parallel pasting compositions of coherence 2-cells in a pseudofunctor agree.                                                                                                                                                                                                                                                                                                                                        | Cited from file 34 Module C and file 37 §VI.8 as the input that licenses "we may treat the coherence data informally — every diagram of `c`'s commutes". Removes the need to verify higher-order coherences explicitly.                                                                                                                                                                                            | 34, 37                     | Mac Lane, _Natural associativity and commutativity_, Rice University Studies **49** (1963); Mac Lane, _Categories for the Working Mathematician_, Ch. VII §2 |
| `strictification`                                 | Strictification of pseudofunctors                                    | Every pseudofunctor `ϕ : ℰ° → Cat` is equivalent (in the 2-category of pseudofunctors) to a strict 2-functor `ϕ̃ : ℰ° → Cat`, after replacing each `ϕ(S)` with an equivalent category. Equivalently: every fibred `ℰ`-category is `ℰ`-equivalent to a split category. The strictification is constructive but enlarges the source data, so working with the un-strictified pseudofunctor is often preferable in practice.                                                                                                                                                                                                                                                                 | Cited from file 37 §VI.9 as the categorical-coherence answer to "do splittings of fibred categories always exist?" — they do up to equivalence, but not on the nose. The result underwrites the slogan "every fibred category is equivalent to a split one".                                                                                                                                                       | 37                         | Power, _A general coherence result_, J. Pure Appl. Algebra **57** (1989); Riehl, _Categorical Homotopy Theory_, §10                                          |

## How to cite

In body prose, name the black box at first mention in each section ("By Zariski's Main Theorem, …", "Serre's criterion
transfers across the étale extension"), then refer to it by name thereafter. Body files use file-level links of the form
`[name](appendix-black-boxes.md)` to point readers here; readers then locate the row by searching for the slug or the
black-box name. The glossary entries for Zariski's Main Theorem, Cohen-Macaulay, Serre's criterion, Nagata ring,
Hensel's lemma, and universal homeomorphism point at the corresponding rows here.

## On EGA citations

Several rows cite parts of EGA that are still being translated. The in-progress English translation lives at
`~/Code/ega/`. As of this writing the chapters most directly relevant to Exposé I — EGA IV §16 (differentials), EGA IV
§17 (smooth, unramified, étale), and EGA 0 §10 (flat-module criteria) — are translated; EGA IV §18 (Zariski's Main
Theorem) is mostly outstanding. For black boxes whose source is outside the translated set we give the EGA reference in
the form Grothendieck himself would; the gentle reading treats them as black boxes regardless.
