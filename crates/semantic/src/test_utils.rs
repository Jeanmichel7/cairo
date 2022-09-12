use std::sync::Arc;

use defs::db::{AsDefsGroup, DefsDatabase, DefsGroup};
use defs::ids::{FreeFunctionId, GenericFunctionId, ModuleId};
use filesystem::db::{init_files_group, AsFilesGroup, FilesDatabase, FilesGroup, FilesGroupEx};
use filesystem::ids::{CrateLongId, Directory};
use parser::db::ParserDatabase;
use pretty_assertions::assert_eq;
use syntax::node::db::{AsSyntaxGroup, SyntaxDatabase, SyntaxGroup};
use utils::extract_matches;

use crate::db::{SemanticDatabase, SemanticGroup};
use crate::{semantic, ExprBlock, ExprId};

#[salsa::database(SemanticDatabase, DefsDatabase, ParserDatabase, SyntaxDatabase, FilesDatabase)]
pub struct SemanticDatabaseForTesting {
    storage: salsa::Storage<SemanticDatabaseForTesting>,
}
impl salsa::Database for SemanticDatabaseForTesting {}
impl Default for SemanticDatabaseForTesting {
    fn default() -> Self {
        let mut res = Self { storage: Default::default() };
        init_files_group(&mut res);
        res
    }
}
impl AsFilesGroup for SemanticDatabaseForTesting {
    fn as_files_group(&self) -> &(dyn FilesGroup + 'static) {
        self
    }
    fn as_files_group_mut(&mut self) -> &mut (dyn FilesGroup + 'static) {
        self
    }
}
impl AsSyntaxGroup for SemanticDatabaseForTesting {
    fn as_syntax_group(&self) -> &(dyn SyntaxGroup + 'static) {
        self
    }
}
impl AsDefsGroup for SemanticDatabaseForTesting {
    fn as_defs_group(&self) -> &(dyn DefsGroup + 'static) {
        self
    }
}

pub struct WithStringDiagnostics<T> {
    value: T,
    diagnostics: String,
}
impl<T> WithStringDiagnostics<T> {
    /// Verifies that there are no diagnostics (fails otherwise), and returns the inner value.
    pub fn unwrap(self) -> T {
        assert_eq!(self.diagnostics, "");
        self.value
    }

    /// Returns the inner value and the diagnostics (as a string).
    pub fn split(self) -> (T, String) {
        (self.value, self.diagnostics)
    }

    /// Returns the diagnostics (as a string).
    pub fn get_diagnostics(self) -> String {
        self.diagnostics
    }
}

/// Helper struct for the return value of [setup_test_module].
pub struct TestModule {
    pub module_id: ModuleId,
}

/// Sets up a module with given content, and returns its module id.
pub fn setup_test_module(
    db: &mut (dyn SemanticGroup + 'static),
    content: &str,
) -> WithStringDiagnostics<TestModule> {
    let crate_id = db.intern_crate(CrateLongId("test_crate".into()));
    let directory = Directory("src".into());
    db.set_crate_root(crate_id, Some(directory));
    let file_id = db.module_file(ModuleId::CrateRoot(crate_id)).unwrap();
    db.as_files_group_mut().override_file_content(file_id, Some(Arc::new(content.to_string())));
    let module_id = ModuleId::CrateRoot(crate_id);

    let syntax_diagnostics = db.file_syntax_diagnostics(file_id).format(db.as_files_group());
    let semantic_diagnostics = db.module_semantic_diagnostics(module_id).unwrap().format(db);

    WithStringDiagnostics {
        value: TestModule { module_id },
        diagnostics: format!("{syntax_diagnostics}{semantic_diagnostics}"),
    }
}

/// Helper struct for the return value of [setup_test_function].
pub struct TestFunction {
    pub module_id: ModuleId,
    pub function_id: FreeFunctionId,
    pub function: semantic::FreeFunction,
}

/// Returns the semantic model of a given function.
/// function_name - name of the function.
/// module_code - extra setup code in the module context.
pub fn setup_test_function(
    db: &mut (dyn SemanticGroup + 'static),
    function_code: &str,
    function_name: &str,
    module_code: &str,
) -> WithStringDiagnostics<TestFunction> {
    let content = if module_code.is_empty() {
        function_code.to_string()
    } else {
        format!("{module_code}\n{function_code}")
    };
    let (test_module, diagnostics) = setup_test_module(db, &content).split();
    let generic_function_id = db
        .module_item_by_name(test_module.module_id, function_name.into())
        .and_then(GenericFunctionId::from)
        .unwrap();
    let function_id = extract_matches!(generic_function_id, GenericFunctionId::Free);
    WithStringDiagnostics {
        value: TestFunction {
            module_id: test_module.module_id,
            function_id,
            function: db.free_function_semantic(function_id).unwrap(),
        },
        diagnostics,
    }
}

/// Helper struct for the return value of [setup_test_expr] and [setup_test_block].
pub struct TestExpr {
    pub module_id: ModuleId,
    pub function_id: FreeFunctionId,
    pub function: semantic::FreeFunction,
    pub expr_id: ExprId,
}

/// Returns the semantic model of a given expression.
/// module_code - extra setup code in the module context.
/// function_body - extra setup code in the function context.
pub fn setup_test_expr(
    db: &mut (dyn SemanticGroup + 'static),
    expr_code: &str,
    module_code: &str,
    function_body: &str,
) -> WithStringDiagnostics<TestExpr> {
    let function_code = format!("func test_func() {{ {function_body} {{\n{expr_code}\n}} }}");
    let (test_function, diagnostics) =
        setup_test_function(db, &function_code, "test_func", module_code).split();
    let ExprBlock { tail: function_body_tail, .. } = extract_matches!(
        db.lookup_intern_expr(test_function.function.body),
        semantic::Expr::ExprBlock
    );
    let ExprBlock { statements, tail, .. } = extract_matches!(
        db.lookup_intern_expr(function_body_tail.unwrap()),
        semantic::Expr::ExprBlock
    );
    assert!(
        statements.is_empty(),
        "expr_code is not a valid expression. Consider using setup_test_block()."
    );
    WithStringDiagnostics {
        value: TestExpr {
            module_id: test_function.module_id,
            function_id: test_function.function_id,
            function: test_function.function,
            expr_id: tail.unwrap(),
        },
        diagnostics,
    }
}

/// Returns the semantic model of a given block expression.
/// module_code - extra setup code in the module context.
/// function_body - extra setup code in the function context.
pub fn setup_test_block(
    db: &mut (dyn SemanticGroup + 'static),
    expr_code: &str,
    module_code: &str,
    function_body: &str,
) -> WithStringDiagnostics<TestExpr> {
    setup_test_expr(db, &format!("{{ {expr_code} }}"), module_code, function_body)
}
