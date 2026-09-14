//! What a WGSL module from the network must be before the window compiles it.
//!
//! `spec/00-rationale.md` refuses arbitrary code from the network and makes
//! one exception: bytecode that passes the verifier in 07. This crate is the
//! second exception, and it is written to be the same kind of thing — a
//! total, shape-only check with a published table of refusals, not a
//! best-effort scan.
//!
//! Three properties are proved here, and nothing else is claimed:
//!
//! 1. **The module terminates**, in at most [`limits::MAX_STEPS`] steps per
//!    invocation. WGSL forbids recursion and naga rejects call cycles, so
//!    the only way a shader can fail to finish is a loop, and every loop
//!    admitted here has a trip count computed from constants.
//! 2. **The module reaches nothing.** No storage, no atomics, no barriers,
//!    no images, no subgroup or ray-query operations: the only thing bound
//!    to it is the client's own uniform block, at the one group and binding
//!    the client offers.
//! 3. **The module has the shape the client compiles.** One fragment entry
//!    named `fs_main`, at most one vertex entry named `vs_main`, and no
//!    compute stage at all.
//!
//! What is *not* proved, and what `spec/11-shaders.md` has to say plainly:
//! a module that passes can still be slow enough to trip a driver's watchdog.
//! The step bound is per invocation; the number of invocations is the size of
//! the target, which the client chooses, and the cost of a step is the
//! hardware's. Verification makes the input to the platform's shader compiler
//! small and dull. It does not make that compiler safe, and it cannot: the
//! compiler is the driver's, and the driver is in the window process.
//!
//! This crate runs in the **worker** (08 §10), which is the point: the parse
//! that meets bytes a server chose happens under Landlock and seccomp, beside
//! the image decoders and the bytecode VM, and not beside TLS and the pin
//! store.

#![forbid(unsafe_code)]

mod limits;

pub use limits::*;

use std::collections::HashMap;
use std::fmt;

use naga::{BinaryOperator, Block, Expression, Function, Handle, Literal, Module, Statement};

/// Why a module was refused.
///
/// Every variant is a vector in `spec/09-conformance.md`: the verdicts are
/// the conformance surface for scenes, because the pixels cannot be.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Reject {
    /// The source is longer than [`MAX_SOURCE_BYTES`].
    TooLong(usize),
    /// WGSL that does not parse. The message is naga's, for a log — never
    /// for the server, which would learn the client's front end from it.
    Parse(String),
    /// Parsed, but not valid WGSL, or valid only with a capability this
    /// client does not enable.
    Invalid(String),
    /// More of something than [`limits`] allows: functions, statements,
    /// expressions, types.
    TooMuch {
        /// What there was too much of.
        what: &'static str,
        /// How many the module has.
        found: usize,
        /// How many it may have.
        allowed: usize,
    },
    /// The entry points are not the ones the client compiles.
    Entry(&'static str),
    /// A global the client does not offer, or offers elsewhere.
    Binding(&'static str),
    /// A statement whose whole purpose is to reach outside the invocation.
    Forbidden(&'static str),
    /// A loop whose trip count cannot be read off its own constants.
    Unbounded(&'static str),
    /// It terminates, but not soon enough: more than [`MAX_STEPS`].
    TooManySteps(u64),
}

impl fmt::Display for Reject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLong(n) => write!(f, "shader source is {n} bytes, over {MAX_SOURCE_BYTES}"),
            Self::Parse(e) => write!(f, "shader does not parse: {e}"),
            Self::Invalid(e) => write!(f, "shader is not valid: {e}"),
            Self::TooMuch { what, found, allowed } => write!(f, "shader has {found} {what}, over {allowed}"),
            Self::Entry(e) => write!(f, "shader entry points: {e}"),
            Self::Binding(e) => write!(f, "shader bindings: {e}"),
            Self::Forbidden(e) => write!(f, "shader uses {e}, which a scene may not"),
            Self::Unbounded(e) => write!(f, "shader loop is not bounded: {e}"),
            Self::TooManySteps(n) => write!(f, "shader takes up to {n} steps, over {MAX_STEPS}"),
        }
    }
}

impl std::error::Error for Reject {}

/// What a module turned out to be, once it passed.
///
/// Deliberately small. It carries what the client needs in order to build a
/// pipeline, and nothing that would let a caller skip the verifier and act
/// on the module itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shape {
    /// The module brings its own vertex stage rather than taking the
    /// client's.
    pub has_vertex: bool,
    /// The worst-case steps one fragment invocation takes — what was proved,
    /// kept so a budget can be measured against what was promised.
    pub steps: u64,
}

/// What a shader asset begins with.
///
/// A module travels in a container rather than as bare WGSL, because 03 §1
/// requires an asset to be told apart "by their first bytes and never by a
/// name or a header the server sent" -- the same rule that gives a mesh its
/// `"EUIM"` and a chunk its `"EUIC"`.
pub const MAGIC: [u8; 4] = *b"EUIS";

/// The only container version there is.
pub const VERSION: u8 = 1;

/// Whether these bytes claim to be a shader, by their first bytes alone.
#[must_use]
pub fn looks_like_shader(bytes: &[u8]) -> bool {
    bytes.get(..4) == Some(&MAGIC)
}

/// Unwrap a shader asset and check what is inside it.
///
/// # Errors
///
/// [`Reject`] names the rule that refused it, including a container that is
/// not one or a version this client does not know.
pub fn verify_asset(bytes: &[u8]) -> Result<(Shape, &str), Reject> {
    if !looks_like_shader(bytes) || bytes.get(4) != Some(&VERSION) {
        return Err(Reject::Parse("not an EUI shader".to_owned()));
    }
    let body = bytes.get(5..).ok_or_else(|| Reject::Parse("truncated".to_owned()))?;
    let source = std::str::from_utf8(body).map_err(|_| Reject::Parse("not UTF-8".to_owned()))?;
    Ok((verify(source)?, source))
}

/// Wrap WGSL in its container, for tests and for a server written in Rust.
#[must_use]
pub fn wrap(source: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(source.len().saturating_add(5));
    out.extend_from_slice(&MAGIC);
    out.push(VERSION);
    out.extend_from_slice(source.as_bytes());
    out
}

/// Check `source`, and say what it is.
///
/// # Errors
///
/// [`Reject`] names the rule that refused it. There is one refusal per rule
/// and no partial acceptance: a module is compiled as it was sent or not at
/// all.
pub fn verify(source: &str) -> Result<Shape, Reject> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(Reject::TooLong(source.len()));
    }
    let module = naga::front::wgsl::parse_str(source).map_err(|e| Reject::Parse(e.message().to_owned()))?;

    // naga first, and it is most of the classic list: types, control flow,
    // uniformity for the derivatives, struct layouts, and — with an empty
    // capability set — f64, push constants, subgroups, ray queries, 64-bit
    // integers, multiview, early depth test, dual source blending and
    // non-uniform indexing, each refused because this client enables none
    // of them on its device. Recursion is refused here too, by the language
    // rather than by us.
    naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::empty()).validate(&module).map_err(|e| Reject::Invalid(e.emit_to_string(source)))?;

    shape_caps(&module)?;
    indexing(&module)?;
    bindings(&module)?;
    let has_vertex = entries(&module)?;
    let steps = cost(&module)?;
    Ok(Shape { has_vertex, steps })
}

/// The module is no bigger than the client will look at.
fn shape_caps(m: &Module) -> Result<(), Reject> {
    let too = |what, found, allowed| Err(Reject::TooMuch { what, found, allowed });
    let functions = m.functions.iter().count().saturating_add(m.entry_points.len());
    if functions > MAX_FUNCTIONS {
        return too("functions", functions, MAX_FUNCTIONS);
    }
    let types = m.types.iter().count();
    if types > MAX_TYPES {
        return too("types", types, MAX_TYPES);
    }
    let mut exprs = m.global_expressions.iter().count();
    let mut stmts = 0usize;
    for f in every_function(m) {
        exprs = exprs.saturating_add(f.expressions.iter().count());
        stmts = stmts.saturating_add(count_statements(&f.body));
    }
    if exprs > MAX_EXPRESSIONS {
        return too("expressions", exprs, MAX_EXPRESSIONS);
    }
    if stmts > MAX_STATEMENTS {
        return too("statements", stmts, MAX_STATEMENTS);
    }
    Ok(())
}

/// Every function in the module, entry points included.
fn every_function(m: &Module) -> impl Iterator<Item = &Function> {
    m.functions.iter().map(|(_, f)| f).chain(m.entry_points.iter().map(|e| &e.function))
}

fn count_statements(b: &Block) -> usize {
    let mut n = 0usize;
    for s in b.iter() {
        n = n.saturating_add(1);
        match s {
            Statement::Block(inner) | Statement::Loop { body: inner, .. } => n = n.saturating_add(count_statements(inner)),
            Statement::If { accept, reject, .. } => n = n.saturating_add(count_statements(accept)).saturating_add(count_statements(reject)),
            Statement::Switch { cases, .. } => {
                for c in cases {
                    n = n.saturating_add(count_statements(&c.body));
                }
            }
            _ => {}
        }
        if let Statement::Loop { continuing, .. } = s {
            n = n.saturating_add(count_statements(continuing));
        }
    }
    n
}

/// Every index is constant.
///
/// Not redundant with the host's bounds checking, and this is the rule with a
/// real bug behind it. A conforming WebGPU implementation generates
/// **unchecked** indexing on a GLES backend -- the safety is left to GLSL,
/// and GLSL's answer to an out-of-range index is undefined behaviour. Vulkan,
/// Metal and DX12 all restrict; GLES does not, and GLES is what an Android
/// device without Vulkan has.
///
/// `AccessIndex` carries its index as a number in the IR and cannot be out of
/// range; `Access` is the dynamic one, and it is admitted only when the index
/// is an integer outright. That is conservative -- it refuses `a[i]` for a
/// loop counter `i` whose bound this crate has in fact just proved -- and it
/// stays conservative until someone needs it not to be: a scene shader has no
/// server-supplied array to walk, because §2.3 binds it none.
fn indexing(m: &Module) -> Result<(), Reject> {
    for f in every_function(m) {
        for (_, e) in f.expressions.iter() {
            if let Expression::Access { index, .. } = e {
                if int_literal(f, *index).is_none() {
                    return Err(Reject::Forbidden("an index that is not a constant"));
                }
            }
        }
    }
    Ok(())
}

/// The only thing bound to a scene shader is the client's uniform block.
///
/// Refusing every other address space is what makes the rest of the checks
/// worth doing: with no storage buffer, no image and no atomic, a shader has
/// nowhere to put a result except its own colour, and nothing to read except
/// what the client handed it.
fn bindings(m: &Module) -> Result<(), Reject> {
    use naga::AddressSpace as A;
    for (_, g) in m.global_variables.iter() {
        match g.space {
            A::Private => {
                if g.binding.is_some() {
                    return Err(Reject::Binding("a private variable may not be bound"));
                }
                continue;
            }
            A::Uniform => {}
            A::Storage { .. } => return Err(Reject::Forbidden("a storage buffer")),
            A::WorkGroup => return Err(Reject::Forbidden("workgroup memory")),
            A::PushConstant => return Err(Reject::Forbidden("a push constant")),
            A::Handle => return Err(Reject::Forbidden("a texture or a sampler")),
            A::Function => return Err(Reject::Binding("a function variable at module scope")),
        }
        let Some(b) = &g.binding else {
            return Err(Reject::Binding("a uniform with no binding"));
        };
        if b.group != UNIFORM_GROUP || b.binding != UNIFORM_BINDING {
            return Err(Reject::Binding("the only binding a scene has is group 0 binding 0"));
        }
        let Ok(ty) = m.types.get_handle(g.ty) else {
            return Err(Reject::Binding("a uniform of no type"));
        };
        if !matches!(ty.inner, naga::TypeInner::Struct { .. }) {
            return Err(Reject::Binding("the uniform block is a struct"));
        }
        // Size rather than name: the block is the client's, so a module that
        // declared one of its own is caught here and not by the driver.
        if ty.inner.size(m.to_ctx()) != UNIFORM_BYTES {
            return Err(Reject::Binding("the uniform block is not the one the client offers"));
        }
    }
    Ok(())
}

/// One fragment stage under the name the client compiles, at most one vertex
/// stage, and no compute stage.
///
/// The names are fixed and the fragment's result is fixed, so no `@location`
/// and no identifier that the server chose ever reaches pipeline creation.
fn entries(m: &Module) -> Result<bool, Reject> {
    let (mut fragment, mut vertex) = (0u32, 0u32);
    for e in &m.entry_points {
        match e.stage {
            naga::ShaderStage::Compute => return Err(Reject::Forbidden("a compute stage")),
            naga::ShaderStage::Fragment => {
                if e.name != FRAGMENT_ENTRY {
                    return Err(Reject::Entry("the fragment stage is named fs_main"));
                }
                fragment = fragment.saturating_add(1);
                let Some(r) = &e.function.result else {
                    return Err(Reject::Entry("the fragment stage returns a colour"));
                };
                if !matches!(r.binding, Some(naga::Binding::Location { location: 0, .. })) {
                    return Err(Reject::Entry("the fragment stage returns @location(0)"));
                }
                let Ok(ty) = m.types.get_handle(r.ty) else {
                    return Err(Reject::Entry("the fragment result has no type"));
                };
                if !matches!(ty.inner, naga::TypeInner::Vector { size: naga::VectorSize::Quad, scalar: naga::Scalar { kind: naga::ScalarKind::Float, .. } }) {
                    return Err(Reject::Entry("the fragment stage returns vec4<f32>"));
                }
            }
            naga::ShaderStage::Vertex => {
                if e.name != VERTEX_ENTRY {
                    return Err(Reject::Entry("the vertex stage is named vs_main"));
                }
                vertex = vertex.saturating_add(1);
            }
        }
        if e.early_depth_test.is_some() {
            return Err(Reject::Forbidden("an early depth test"));
        }
    }
    if fragment != 1 {
        return Err(Reject::Entry("exactly one fragment stage"));
    }
    if vertex > 1 {
        return Err(Reject::Entry("at most one vertex stage"));
    }
    Ok(vertex == 1)
}

/// The worst-case steps the module takes, proved from its own constants.
///
/// Functions are costed once each and in arena order, which is declaration
/// order: WGSL requires a callee to be declared before its caller, so a
/// function's cost is always known by the time a call to it is counted.
fn cost(m: &Module) -> Result<u64, Reject> {
    let mut costs: HashMap<usize, u64> = HashMap::new();
    for (h, f) in m.functions.iter() {
        let c = block_cost(&f.body, f, &costs)?;
        costs.insert(h.index(), c);
    }
    let mut worst = 0u64;
    for e in &m.entry_points {
        let c = block_cost(&e.function.body, &e.function, &costs)?;
        worst = worst.max(c);
    }
    if worst > MAX_STEPS {
        return Err(Reject::TooManySteps(worst));
    }
    Ok(worst)
}

fn block_cost(b: &Block, f: &Function, costs: &HashMap<usize, u64>) -> Result<u64, Reject> {
    let mut n = 0u64;
    let stmts: &[Statement] = b;
    for (at, s) in stmts.iter().enumerate() {
        n = n.saturating_add(1);
        n = n.saturating_add(match s {
            // The statements a scene may not have. Each is refused in one
            // place so there is one vector per rule.
            Statement::Atomic { .. } => return Err(Reject::Forbidden("an atomic")),
            Statement::ImageStore { .. } => return Err(Reject::Forbidden("an image store")),
            Statement::Barrier(_) => return Err(Reject::Forbidden("a barrier")),
            Statement::WorkGroupUniformLoad { .. } => return Err(Reject::Forbidden("a workgroup load")),
            Statement::RayQuery { .. } => return Err(Reject::Forbidden("a ray query")),
            Statement::SubgroupBallot { .. } | Statement::SubgroupGather { .. } | Statement::SubgroupCollectiveOperation { .. } => return Err(Reject::Forbidden("a subgroup operation")),

            Statement::Block(inner) => block_cost(inner, f, costs)?,
            // Both arms, because which one runs is not known here and the
            // number wanted is the worst case.
            Statement::If { accept, reject, .. } => block_cost(accept, f, costs)?.max(block_cost(reject, f, costs)?),
            Statement::Switch { cases, .. } => {
                let mut worst = 0u64;
                for c in cases {
                    worst = worst.max(block_cost(&c.body, f, costs)?);
                }
                worst
            }
            // WGSL requires a callee to be declared before its caller, so
            // the memo is filled by the time a call is costed. A miss would
            // mean the module is shaped in a way this pass did not expect,
            // and the safe answer to that is "too much".
            Statement::Call { function, .. } => costs.get(&function.index()).copied().unwrap_or(MAX_STEPS.saturating_add(1)),
            Statement::Loop { body, continuing, break_if } => {
                // What runs before the loop in its own block, which is where
                // naga leaves a nested counter's initial value.
                let before = stmts.get(..at).unwrap_or(&[]);
                let trips = loop_trips(body, continuing, *break_if, f, before)?;
                let once = block_cost(body, f, costs)?.saturating_add(block_cost(continuing, f, costs)?);
                trips.saturating_mul(once.max(1))
            }
            _ => 0,
        });
        if n > MAX_STEPS {
            return Err(Reject::TooManySteps(n));
        }
    }
    Ok(n)
}

/// How many times a loop can run, read off its own constants.
///
/// This is the rule the whole crate exists for, so it is worth saying what
/// it accepts and why the form is so narrow. naga lowers both `for` and
/// `while` to the same thing: a `Loop` whose body opens with a conditional
/// `break`, and whose `continuing` block holds the step. So the shape below
/// is not "a `for` loop" — it is *every* loop whose bound can be read
/// without running it:
///
/// ```wgsl
/// for (var i: i32 = 0; i < 8; i = i + 1) { … }
/// ```
///
/// Four facts have to line up, and if any one of them is missing the loop is
/// refused rather than guessed at:
///
/// - the counter is a local with a constant initialiser;
/// - `continuing` holds exactly one store, of the counter plus or minus a
///   non-zero constant;
/// - the body opens with a break on a comparison of that counter against a
///   constant;
/// - and nothing in the body assigns the counter, which would make all of
///   the above describe a loop that is not the one running.
///
/// A bound that comes from a uniform is refused too, and that is the rule
/// people will argue with. It has to be: a uniform is a number the server
/// sends, so a bound read from one is a bound the server sets after the
/// module was verified, which is no bound at all.
fn loop_trips(body: &Block, continuing: &Block, break_if: Option<Handle<Expression>>, f: &Function, before: &[Statement]) -> Result<u64, Reject> {
    if break_if.is_some() {
        // `continuing { break if … }` tests after the step, so the count is
        // off by one from the form below. Refusing it costs an author one
        // rewrite and saves a rule that would be right only sometimes.
        return Err(Reject::Unbounded("a `break if` in a continuing block"));
    }

    // The step: exactly one store, of the counter, by a constant.
    let mut step: Option<(Handle<naga::LocalVariable>, i64)> = None;
    for s in continuing.iter() {
        match s {
            Statement::Emit(_) => {}
            Statement::Store { pointer, value } => {
                if step.is_some() {
                    return Err(Reject::Unbounded("a continuing block that assigns twice"));
                }
                let Some(v) = local_of(f, *pointer) else {
                    return Err(Reject::Unbounded("a step that is not a local"));
                };
                let Some(k) = step_of(f, *value, v) else {
                    return Err(Reject::Unbounded("a step that is not the counter plus a constant"));
                };
                if k == 0 {
                    return Err(Reject::Unbounded("a step of zero"));
                }
                step = Some((v, k));
            }
            _ => return Err(Reject::Unbounded("a continuing block that does more than step")),
        }
    }
    let Some((counter, k)) = step else {
        return Err(Reject::Unbounded("a loop with no step"));
    };

    // The test: a conditional break, first thing in the body.
    let mut test = None;
    for s in body.iter() {
        match s {
            Statement::Emit(_) => {}
            Statement::If { condition, accept, reject } => {
                test = Some((*condition, is_break(accept), is_break(reject)));
                break;
            }
            _ => break,
        }
    }
    let Some((condition, breaks_when_true, breaks_when_false)) = test else {
        return Err(Reject::Unbounded("a loop that does not open with a conditional break"));
    };
    // Exactly one arm breaks, and the other does nothing: anything else and
    // the comparison below is not the loop's exit.
    let go_on = match (breaks_when_true, breaks_when_false) {
        (Some(true), Some(false)) => false, // breaks when the condition holds
        (Some(false), Some(true)) => true,  // runs on while it holds
        _ => return Err(Reject::Unbounded("a conditional break with a body on both arms")),
    };

    let Some((mut op, limit)) = compare_of(f, condition, counter) else {
        return Err(Reject::Unbounded("a test that is not the counter against a constant"));
    };
    if !go_on {
        op = negate(op);
    }

    // The start, and the promise that nothing else moves the counter.
    let Some(init) = start_of(f, counter, before) else {
        return Err(Reject::Unbounded("a counter with no constant start"));
    };
    if assigns(body, f, counter) {
        return Err(Reject::Unbounded("a counter the body also assigns"));
    }

    let trips = trips_of(init, limit, k, op).ok_or(Reject::Unbounded("a counter that never reaches its limit"))?;
    if trips > MAX_LOOP_TRIPS {
        return Err(Reject::TooManySteps(trips));
    }
    Ok(trips)
}

/// The value a counter starts a loop with.
///
/// naga writes it in one of two places, and which one is not the author's
/// doing: the outermost `for` in a function gets its initialiser folded into
/// the `LocalVariable`, and every nested one gets a plain store in front of
/// the loop instead. Reading only the first place would have refused every
/// nested loop — which is to say, exactly the loops whose trip counts
/// multiply, and the ones this analysis exists for.
///
/// The nearest store wins, and if that store is not a constant the answer is
/// `None`: a counter whose start the server picks is not a counter that was
/// bounded here.
fn start_of(f: &Function, counter: Handle<naga::LocalVariable>, before: &[Statement]) -> Option<i64> {
    for s in before.iter().rev() {
        if let Statement::Store { pointer, value } = s {
            if local_of(f, *pointer) == Some(counter) {
                return int_literal(f, *value);
            }
        }
    }
    f.local_variables.try_get(counter).ok()?.init.and_then(|e| int_literal(f, e))
}

/// `Some(true)` for a block that is exactly `break`, `Some(false)` for an
/// empty one, `None` for anything else.
fn is_break(b: &Block) -> Option<bool> {
    let real: Vec<_> = b.iter().filter(|s| !matches!(s, Statement::Emit(_))).collect();
    match real.as_slice() {
        [] => Some(false),
        [Statement::Break] => Some(true),
        _ => None,
    }
}

fn local_of(f: &Function, e: Handle<Expression>) -> Option<Handle<naga::LocalVariable>> {
    match f.expressions.try_get(e).ok()? {
        Expression::LocalVariable(v) => Some(*v),
        _ => None,
    }
}

fn load_of(f: &Function, e: Handle<Expression>) -> Option<Handle<naga::LocalVariable>> {
    match f.expressions.try_get(e).ok()? {
        Expression::Load { pointer } => local_of(f, *pointer),
        _ => None,
    }
}

/// An integer this expression is, if it is one outright.
fn int_literal(f: &Function, e: Handle<Expression>) -> Option<i64> {
    match f.expressions.try_get(e).ok()? {
        Expression::Literal(Literal::I32(n)) => Some(i64::from(*n)),
        Expression::Literal(Literal::U32(n)) => Some(i64::from(*n)),
        Expression::Literal(Literal::AbstractInt(n)) => Some(*n),
        _ => None,
    }
}

/// The constant a store adds to `counter`, for a value of the form
/// `counter + k` or `counter - k` — in either operand order for the sum.
fn step_of(f: &Function, value: Handle<Expression>, counter: Handle<naga::LocalVariable>) -> Option<i64> {
    let Expression::Binary { op, left, right } = f.expressions.try_get(value).ok()? else {
        return None;
    };
    let is_counter = |e| load_of(f, e) == Some(counter);
    match op {
        BinaryOperator::Add if is_counter(*left) => int_literal(f, *right),
        BinaryOperator::Add if is_counter(*right) => int_literal(f, *left),
        BinaryOperator::Subtract if is_counter(*left) => int_literal(f, *right).and_then(i64::checked_neg),
        _ => None,
    }
}

/// The comparison a condition is, normalised so the counter is on the left.
fn compare_of(f: &Function, condition: Handle<Expression>, counter: Handle<naga::LocalVariable>) -> Option<(BinaryOperator, i64)> {
    let Expression::Binary { op, left, right } = f.expressions.try_get(condition).ok()? else {
        return None;
    };
    if load_of(f, *left) == Some(counter) {
        return int_literal(f, *right).map(|l| (*op, l));
    }
    if load_of(f, *right) == Some(counter) {
        return int_literal(f, *left).map(|l| (mirror(*op), l));
    }
    None
}

/// The same comparison with its operands swapped.
fn mirror(op: BinaryOperator) -> BinaryOperator {
    match op {
        BinaryOperator::Less => BinaryOperator::Greater,
        BinaryOperator::LessEqual => BinaryOperator::GreaterEqual,
        BinaryOperator::Greater => BinaryOperator::Less,
        BinaryOperator::GreaterEqual => BinaryOperator::LessEqual,
        other => other,
    }
}

/// The comparison that is true exactly when this one is false.
fn negate(op: BinaryOperator) -> BinaryOperator {
    match op {
        BinaryOperator::Less => BinaryOperator::GreaterEqual,
        BinaryOperator::LessEqual => BinaryOperator::Greater,
        BinaryOperator::Greater => BinaryOperator::LessEqual,
        BinaryOperator::GreaterEqual => BinaryOperator::Less,
        BinaryOperator::Equal => BinaryOperator::NotEqual,
        BinaryOperator::NotEqual => BinaryOperator::Equal,
        other => other,
    }
}

/// Whether any store in `b`, however deep, targets `v`.
fn assigns(b: &Block, f: &Function, v: Handle<naga::LocalVariable>) -> bool {
    b.iter().any(|s| match s {
        Statement::Store { pointer, .. } => local_of(f, *pointer) == Some(v),
        Statement::Block(inner) => assigns(inner, f, v),
        Statement::If { accept, reject, .. } => assigns(accept, f, v) || assigns(reject, f, v),
        Statement::Switch { cases, .. } => cases.iter().any(|c| assigns(&c.body, f, v)),
        Statement::Loop { body, continuing, .. } => assigns(body, f, v) || assigns(continuing, f, v),
        _ => false,
    })
}

/// How many times `start`, stepped by `k`, satisfies `counter op limit`.
///
/// `None` is a counter that never stops satisfying it — the unbounded loop
/// this whole function exists to catch.
fn trips_of(start: i64, limit: i64, k: i64, op: BinaryOperator) -> Option<u64> {
    // Upward and downward are the same arithmetic mirrored, so both are
    // written as "how far to go, divided by how fast, rounded up".
    let up = |to: i64| -> Option<u64> {
        if k <= 0 {
            return None;
        }
        let gap = to.checked_sub(start)?;
        if gap <= 0 {
            return Some(0);
        }
        let ceil = gap.checked_add(k.checked_sub(1)?)?.checked_div(k)?;
        u64::try_from(ceil).ok()
    };
    let down = |to: i64| -> Option<u64> {
        if k >= 0 {
            return None;
        }
        let gap = start.checked_sub(to)?;
        if gap <= 0 {
            return Some(0);
        }
        let by = k.checked_neg()?;
        let ceil = gap.checked_add(by.checked_sub(1)?)?.checked_div(by)?;
        u64::try_from(ceil).ok()
    };
    match op {
        BinaryOperator::Less => up(limit),
        BinaryOperator::LessEqual => up(limit.checked_add(1)?),
        BinaryOperator::Greater => down(limit),
        BinaryOperator::GreaterEqual => down(limit.checked_sub(1)?),
        // Runs while the counter misses the limit: only a counter that lands
        // on it exactly ever stops.
        BinaryOperator::NotEqual => {
            let gap = limit.checked_sub(start)?;
            if gap == 0 {
                return Some(0);
            }
            if gap.checked_rem(k)? != 0 || (gap > 0) != (k > 0) {
                return None;
            }
            u64::try_from(gap.checked_div(k)?).ok()
        }
        // Runs while the counter equals the limit, which a non-zero step
        // makes false on the first step.
        BinaryOperator::Equal => Some(u64::from(start == limit)),
        _ => None,
    }
}
