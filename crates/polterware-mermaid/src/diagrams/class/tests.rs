//! The upstream class specs as Rust tests:
//!
//! - `classDiagram.spec.ts` (parse and database state)
//! - `classTypes.spec.ts` (`ClassMember` parsing and display)
//! - `parser/class.spec.js` (`__proto__`/`constructor` names, backticked namespace)
//! - `classDiagram-styles.spec.js` (css classes, `classDef`, `style`)
//!
//! Skipped, with the upstream name and the reason:
//!
//! - `classDiagram.spec.ts` "emits only explicit namespaces from getData() in compact
//!   mode" and "moves classes inside implicit namespaces to the nearest explicit
//!   ancestor in compact mode": they set `class.hierarchicalNamespaces`, which is config,
//!   and read `getData()`, which is rendering data.
//! - `classDiagram.spec.ts` "defaults to hierarchical rendering when
//!   hierarchicalNamespaces is unset": ported as a namespace assertion (label and parent)
//!   without `getData()`'s nodes.
//! - `classDiagram.spec.ts` "should handle a leading newline": skipped upstream too
//!   (`it.skip`), and the preprocessed body never carries the skipped line.
//! - `classDiagram.spec.ts` "should have functions used in class JISON as own property":
//!   it pins Jison's method binding, not a database value.
//! - `classDiagram.spec.ts` spy assertions on `setLink`/`setTooltip`/`setClickEvent`
//!   arguments: the values those calls store (`link`, `link_target`, `tooltip`,
//!   `haveCallback`, `cssClasses`) are asserted instead; the `call` arguments only feed a
//!   DOM handler this crate does not have.
//! - `classDiagram.spec.ts` inline snapshot `domId: "classId-Student-154"`: the number
//!   comes from upstream's module-global `classCounter` and depends on the tests that ran
//!   before it.
//! - `classDiagram-colorIndex.spec.ts`: `colorIndex` is assigned in `getData()`, which is
//!   render data.

use super::*;
use pretty_assertions::assert_eq;

fn parsed(body: &str) -> Ast {
    parse(body).unwrap_or_else(|error| panic!("parse failed for {body:?}: {error}"))
}

fn display(input: &str, member_type: MemberType) -> DisplayDetails {
    ClassMember::new(input, member_type).display_details()
}

const KEYWORDS: &[&str] = &[
    "direction",
    "classDiagram",
    "classDiagram-v2",
    "namespace",
    "{}",
    "{",
    "}",
    "()",
    "(",
    ")",
    "[]",
    "[",
    "]",
    "class",
    "\n",
    "cssClass",
    "callback",
    "link",
    "click",
    "note",
    "note for",
    "<<",
    ">>",
    "call ",
    "~",
    "~Generic~",
    "_self",
    "_blank",
    "_parent",
    "_top",
    "<|",
    "|>",
    ">",
    "<",
    "*",
    "o",
    "\\",
    "--",
    "..",
    "-->",
    "--|>",
    ": label",
    ":::",
    ".",
    "+",
    "alphaNum",
    "!",
    "0123",
    "function()",
    "function(arg1, arg2)",
];

// -- classDiagram.spec.ts: "given a basic class diagram, when parsing class definition" --

#[test]
fn handles_classes_within_namespaces() {
    let body = "classDiagram
        namespace Company.Project {
          class User {
            +login(username: String, password: String)
            +logout()
          }
        }
        namespace Company.Project.Module {
          class Admin {
            +addUser(user: User)
            +removeUser(user: User)
          }
        }";
    let ast = parsed(body);
    let user = ast.get_class("User").expect("User");
    let admin = ast.get_class("Admin").expect("Admin");
    assert_eq!(user.parent.as_deref(), Some("Company.Project"));
    assert_eq!(admin.parent.as_deref(), Some("Company.Project.Module"));
    assert_eq!(user.methods.len(), 2);
    assert_eq!(admin.methods.len(), 2);
}

#[test]
fn handles_generic_classes_within_namespaces() {
    let body = "classDiagram
    namespace Company.Project.Module {
        class GenericClass~T~ {
            +addItem(item: T)
            +getItem() T
        }
    }";
    let ast = parsed(body);
    let class = ast.get_class("GenericClass").expect("GenericClass");
    assert_eq!(class.type_name, "T");
    assert_eq!(
        class.methods[0].display_details().display_text,
        "+addItem(item: T)"
    );
    assert_eq!(
        class.methods[1].display_details().display_text,
        "+getItem() : T"
    );
}

#[test]
fn handles_nested_namespaces_and_relationships() {
    let body = "      classDiagram
        namespace Company.Project.Module.SubModule {
          class Report {
            +generatePDF(data: List)
            +generateCSV(data: List)
          }
        }
        namespace Company.Project.Module {
          class Admin {
            +generateReport()
          }
        }
        Admin --> Report : generates";
    let ast = parsed(body);
    let report = ast.get_class("Report").expect("Report");
    let admin = ast.get_class("Admin").expect("Admin");
    assert_eq!(
        report.parent.as_deref(),
        Some("Company.Project.Module.SubModule")
    );
    assert_eq!(admin.parent.as_deref(), Some("Company.Project.Module"));
    assert_eq!(ast.relations[0].id1, "Admin");
    assert_eq!(ast.relations[0].id2, "Report");
    assert_eq!(ast.relations[0].title.as_deref(), Some("generates"));

    for namespace in [
        "Company",
        "Company.Project",
        "Company.Project.Module",
        "Company.Project.Module.SubModule",
    ] {
        assert!(ast.get_namespaces().contains(namespace), "{namespace}");
    }

    assert_eq!(ast.get_namespace("Company").unwrap().parent, None);
    assert_eq!(
        ast.get_namespace("Company.Project")
            .unwrap()
            .parent
            .as_deref(),
        Some("Company")
    );
    assert_eq!(
        ast.get_namespace("Company.Project.Module")
            .unwrap()
            .parent
            .as_deref(),
        Some("Company.Project")
    );
    assert_eq!(
        ast.get_namespace("Company.Project.Module.SubModule")
            .unwrap()
            .parent
            .as_deref(),
        Some("Company.Project.Module")
    );

    assert!(
        ast.get_namespace("Company")
            .unwrap()
            .children
            .contains(&"Company.Project".to_string())
    );
    assert!(
        ast.get_namespace("Company.Project")
            .unwrap()
            .children
            .contains(&"Company.Project.Module".to_string())
    );
    assert!(
        ast.get_namespace("Company.Project.Module")
            .unwrap()
            .children
            .contains(&"Company.Project.Module.SubModule".to_string())
    );

    assert!(
        ast.get_namespace("Company.Project.Module.SubModule")
            .unwrap()
            .explicit
    );
    assert!(
        ast.get_namespace("Company.Project.Module")
            .unwrap()
            .explicit
    );
    assert!(!ast.get_namespace("Company.Project").unwrap().explicit);
    assert!(!ast.get_namespace("Company").unwrap().explicit);
}

#[test]
fn handles_syntactically_nested_namespace_blocks() {
    let body = "classDiagram
        namespace Outer {
          namespace Inner {
            class Foo {
              +bar()
            }
          }
          class Baz {
            +qux()
          }
        }";
    let ast = parsed(body);
    let foo = ast.get_class("Foo").expect("Foo");
    let baz = ast.get_class("Baz").expect("Baz");
    assert_eq!(foo.parent.as_deref(), Some("Outer.Inner"));
    assert_eq!(baz.parent.as_deref(), Some("Outer"));
    assert!(ast.get_namespaces().contains("Outer"));
    assert!(ast.get_namespaces().contains("Outer.Inner"));
    assert_eq!(
        ast.get_namespace("Outer.Inner").unwrap().parent.as_deref(),
        Some("Outer")
    );
    assert!(
        ast.get_namespace("Outer")
            .unwrap()
            .children
            .contains(&"Outer.Inner".to_string())
    );
    assert!(ast.get_namespace("Outer").unwrap().explicit);
    assert!(ast.get_namespace("Outer.Inner").unwrap().explicit);
}

#[test]
fn defaults_to_hierarchical_namespace_labels_and_parents() {
    let body = "classDiagram
        namespace A.B {
          class Foo
        }";
    let ast = parsed(body);
    let leaf = ast.get_namespace("A.B").expect("A.B");
    assert_eq!(leaf.label, "B");
    assert_eq!(leaf.parent.as_deref(), Some("A"));
    assert!(leaf.explicit);
    let ancestor = ast.get_namespace("A").expect("A");
    assert_eq!(ancestor.label, "A");
    assert!(!ancestor.explicit);
    let ids: Vec<&str> = ast.get_namespaces().keys().collect();
    assert_eq!(ids, ["A", "A.B"]);
}

#[test]
fn supports_custom_labels_on_namespaces() {
    let body = "classDiagram
        namespace Auth[\"Authentication Service\"] {
          class UserService {
            +login()
          }
        }";
    let ast = parsed(body);
    let auth = ast.get_namespace("Auth").expect("Auth");
    assert_eq!(auth.id, "Auth");
    assert_eq!(auth.label, "Authentication Service");
    assert_eq!(
        ast.get_class("UserService").unwrap().parent.as_deref(),
        Some("Auth")
    );
}

#[test]
fn handles_acc_title_and_acc_descr() {
    let body = "classDiagram
            accTitle: My Title
            accDescr: My Description";
    let ast = parsed(body);
    assert_eq!(ast.acc_title.as_deref(), Some("My Title"));
    assert_eq!(ast.acc_description.as_deref(), Some("My Description"));
}

#[test]
fn handles_acc_title_and_multiline_acc_descr() {
    let body = "classDiagram
            accTitle: My Title
            accDescr {
              This is my multi
              line description
            }";
    let ast = parsed(body);
    assert_eq!(ast.acc_title.as_deref(), Some("My Title"));
    assert_eq!(
        ast.acc_description.as_deref(),
        Some("This is my multi\nline description")
    );
}

#[test]
fn handles_fully_qualified_class_names_in_relationships() {
    let body = "classDiagram
        namespace Company.Project.Module {
          class User
          class Admin
        }
          Admin --> User : manages";
    let ast = parsed(body);
    assert_eq!(ast.relations[0].id1, "Admin");
    assert_eq!(ast.relations[0].id2, "User");
    assert_eq!(ast.relations[0].title.as_deref(), Some("manages"));
}

#[test]
fn handles_backticked_class_names() {
    parsed("classDiagram\nclass `Car`");
}

#[test]
fn handles_class_names_with_dash() {
    let ast = parsed("classDiagram\nclass Ca-r");
    assert_eq!(ast.get_class("Ca-r").unwrap().label, "Ca-r");
}

#[test]
fn handles_class_names_with_underscore() {
    parsed("classDiagram\nclass `A_Car`");
}

#[test]
fn handles_parsing_of_separators() {
    let body = "classDiagram\nclass Foo1 {\n  You can use\n  several lines\n..\nas you want\nand group\n==\nthings together.\n__\nYou can have as many groups\nas you want\n--\nEnd of class\n}\n\nclass User {\n.. Simple Getter ..\n+ getName()\n+ getAddress()\n.. Some setter ..\n+ setName()\n__ private data __\nint age\n-- encrypted --\nString password\n}";
    parsed(body);
}

#[test]
fn parses_a_class_with_a_text_label() {
    let ast = parsed("classDiagram\nclass C1[\"Class 1 with text label\"]");
    assert_eq!(
        ast.get_class("C1").unwrap().label,
        "Class 1 with text label"
    );
}

#[test]
fn parses_two_classes_with_text_labels() {
    let ast = parsed(
        "classDiagram\nclass C1[\"Class 1 with text label\"]\nclass C2[\"Class 2 with chars @?\"]\n",
    );
    assert_eq!(
        ast.get_class("C1").unwrap().label,
        "Class 1 with text label"
    );
    assert_eq!(ast.get_class("C2").unwrap().label, "Class 2 with chars @?");
}

#[test]
fn parses_a_class_with_a_text_label_and_member() {
    let ast = parsed("classDiagram\nclass C1[\"Class 1 with text label\"]\nC1: member1");
    let c1 = ast.get_class("C1").unwrap();
    assert_eq!(c1.label, "Class 1 with text label");
    assert_eq!(c1.members.len(), 1);
    assert_eq!(c1.members[0].display_details().display_text, "member1");
}

#[test]
fn parses_a_class_with_a_text_label_member_and_annotation() {
    let ast = parsed(
        "classDiagram\nclass C1[\"Class 1 with text label\"]\n<<interface>> C1\nC1 : int member1",
    );
    let c1 = ast.get_class("C1").unwrap();
    assert_eq!(c1.label, "Class 1 with text label");
    assert_eq!(c1.members.len(), 1);
    assert_eq!(c1.members[0].display_details().display_text, "int member1");
    assert_eq!(c1.annotations.len(), 1);
    assert_eq!(c1.annotations[0], "interface");
}

#[test]
fn parses_a_class_with_text_label_and_css_class_shorthand() {
    let ast = parsed("classDiagram\nclass C1[\"Class 1 with text label\"]:::styleClass");
    let c1 = ast.get_class("C1").unwrap();
    assert_eq!(c1.label, "Class 1 with text label");
    assert_eq!(c1.css_classes, "default styleClass");
}

#[test]
fn parses_a_class_with_text_label_and_css_class() {
    let ast = parsed(
        "classDiagram\nclass C1[\"Class 1 with text label\"]\nC1 : int member1\ncssClass \"C1\" styleClass",
    );
    let c1 = ast.get_class("C1").unwrap();
    assert_eq!(c1.label, "Class 1 with text label");
    assert_eq!(c1.members[0].display_details().display_text, "int member1");
    assert_eq!(c1.css_classes, "default styleClass");
}

#[test]
fn parses_two_classes_with_text_labels_and_css_classes() {
    let ast = parsed(
        "classDiagram\nclass C1[\"Class 1 with text label\"]\nC1 : int member1\nclass C2[\"Long long long long long long long long long long label\"]\ncssClass \"C1,C2\" styleClass",
    );
    let c1 = ast.get_class("C1").unwrap();
    assert_eq!(c1.label, "Class 1 with text label");
    assert_eq!(c1.css_classes, "default styleClass");
    let c2 = ast.get_class("C2").unwrap();
    assert_eq!(
        c2.label,
        "Long long long long long long long long long long label"
    );
    assert_eq!(c2.css_classes, "default styleClass");
}

#[test]
fn parses_two_classes_with_text_labels_and_css_class_shorthands() {
    let ast = parsed(
        "classDiagram\nclass C1[\"Class 1 with text label\"]:::styleClass1\nclass C2[\"Class 2 !@#$%^&*() label\"]:::styleClass2",
    );
    let c1 = ast.get_class("C1").unwrap();
    assert_eq!(c1.label, "Class 1 with text label");
    assert_eq!(c1.css_classes, "default styleClass1");
    let c2 = ast.get_class("C2").unwrap();
    assert_eq!(c2.label, "Class 2 !@#$%^&*() label");
    assert_eq!(c2.css_classes, "default styleClass2");
}

#[test]
fn applies_css_classes_to_generic_classes() {
    let ast = parsed("classDiagram\nclass C1~T~\ncssClass \"C1~T~\" styleClass");
    assert_eq!(
        ast.get_class("C1").unwrap().css_classes,
        "default styleClass"
    );
}

#[test]
fn applies_css_classes_to_generic_classes_via_shorthand() {
    let ast = parsed("classDiagram\nclass C1~T~:::styleClass");
    assert_eq!(
        ast.get_class("C1").unwrap().css_classes,
        "default styleClass"
    );
}

#[test]
fn binds_click_event_on_generic_class() {
    let ast = parsed("classDiagram\nclass C1~T~\nclick C1~T~ call someFunction()");
    assert!(ast.get_class("C1").unwrap().have_callback);
}

#[test]
fn binds_tooltip_on_generic_class() {
    let ast = parsed("classDiagram\nclass C1~T~\nclick C1~T~ call cb() \"a tip\"");
    assert_eq!(
        ast.get_class("C1").unwrap().tooltip.as_deref(),
        Some("a tip")
    );
}

#[test]
fn binds_link_on_generic_class() {
    let ast = parsed("classDiagram\nclass C1~T~\nlink C1~T~ \"https://example.com\"");
    let c1 = ast.get_class("C1").unwrap();
    assert_eq!(c1.link.as_deref(), Some("https://example.com/"));
    assert_eq!(c1.css_classes, "default clickable");
}

#[test]
fn parses_multiple_classes_with_same_text_labels() {
    let ast = parsed(
        "classDiagram\nclass C1[\"Class with text label\"]\nclass C2[\"Class with text label\"]\nclass C3[\"Class with text label\"]",
    );
    for id in ["C1", "C2", "C3"] {
        assert_eq!(ast.get_class(id).unwrap().label, "Class with text label");
    }
}

#[test]
fn parses_classes_with_different_text_labels() {
    let body = "classDiagram
class C1[\"OneWord\"]
class C2[\"With, Comma\"]
class C3[\"With (Brackets)\"]
class C4[\"With [Brackets]\"]
class C5[\"With {Brackets}\"]
class C6[\" \" ]
class C7[\"With 1 number\"]
class C8[\"With . period...\"]
class C9[\"With - dash\"]
class C10[\"With _ underscore\"]
class C11[\"With ' single quote\"]
class C12[\"With ~!@#$%^&*()_+=-/?\"]
class C13[\"With Città foreign language\"]
";
    let ast = parsed(body);
    assert_eq!(ast.get_class("C1").unwrap().label, "OneWord");
    assert_eq!(ast.get_class("C2").unwrap().label, "With, Comma");
    assert_eq!(ast.get_class("C3").unwrap().label, "With (Brackets)");
    assert_eq!(ast.get_class("C4").unwrap().label, "With [Brackets]");
    assert_eq!(ast.get_class("C5").unwrap().label, "With {Brackets}");
    assert_eq!(ast.get_class("C6").unwrap().label, " ");
    assert_eq!(ast.get_class("C7").unwrap().label, "With 1 number");
    assert_eq!(ast.get_class("C8").unwrap().label, "With . period...");
    assert_eq!(ast.get_class("C9").unwrap().label, "With - dash");
    assert_eq!(ast.get_class("C10").unwrap().label, "With _ underscore");
    assert_eq!(ast.get_class("C11").unwrap().label, "With ' single quote");
    assert_eq!(
        ast.get_class("C12").unwrap().label,
        "With ~!@#$%^&*()_+=-/?"
    );
    assert_eq!(
        ast.get_class("C13").unwrap().label,
        "With Città foreign language"
    );
}

#[test]
fn handles_note_for() {
    parsed("classDiagram\nClass11 <|.. Class12\nnote for Class11 \"test\"\n");
}

#[test]
fn handles_note() {
    parsed("classDiagram\nnote \"test\"\n");
}

#[test]
fn notes_keep_keywords_in_text() {
    for keyword in KEYWORDS {
        let body = format!(
            "classDiagram\n                     note \"This is a keyword: {keyword}. It truly is.\"\n                  "
        );
        let ast = parsed(&body);
        assert_eq!(
            ast.get_note(0).unwrap().text,
            format!("This is a keyword: {keyword}. It truly is."),
            "keyword: {keyword:?}"
        );
    }
}

#[test]
fn notes_keep_a_leading_keyword() {
    for keyword in KEYWORDS {
        let body = format!("classDiagram\n                      note \"{keyword}\"");
        let ast = parsed(&body);
        assert_eq!(
            ast.get_note(0).unwrap().text,
            *keyword,
            "keyword: {keyword:?}"
        );
    }
}

#[test]
fn notes_for_keep_keywords_in_text() {
    for keyword in KEYWORDS {
        let body = format!(
            "classDiagram\n                   class Something {{\n                     int id\n                     string name\n                   }}\n                   note for Something \"This is a keyword: {keyword}. It truly is.\"\n                   "
        );
        let ast = parsed(&body);
        assert_eq!(
            ast.get_note(0).unwrap().text,
            format!("This is a keyword: {keyword}. It truly is."),
            "keyword: {keyword:?}"
        );
    }
}

#[test]
fn notes_for_keep_a_leading_keyword() {
    for keyword in KEYWORDS {
        let body = format!(
            "classDiagram\n                    class Something {{\n                      int id\n                      string name\n                    }}\n                    note for Something \"{keyword}\"\n                    "
        );
        let ast = parsed(&body);
        assert_eq!(
            ast.get_note(0).unwrap().text,
            *keyword,
            "keyword: {keyword:?}"
        );
    }
}

#[test]
fn errors_for_a_keyword_after_note() {
    for keyword in KEYWORDS {
        let body = format!("classDiagram\n                   note {keyword}");
        let result = parse(&body);
        assert!(result.is_err(), "keyword: {keyword:?}");
    }
}

#[test]
fn parses_diagram_with_direction() {
    let body = "classDiagram
          direction TB
          class Student {
            -idCard : IdCard
          }
          class IdCard{
            -id : int
            -name : string
          }
          class Bike{
            -id : int
            -name : string
          }
          Student \"1\" --o \"1\" IdCard : carries
          Student \"1\" --o \"1\" Bike : rides";
    let ast = parsed(body);

    let student = ast.get_class("Student").expect("Student");
    assert_eq!(student.id, "Student");
    assert_eq!(student.label, "Student");
    assert_eq!(student.members.len(), 1);
    let member = &student.members[0];
    assert_eq!(member.id, "idCard : IdCard");
    assert_eq!(member.visibility, Visibility::Private);
    assert_eq!(member.text, "\\-idCard : IdCard");
    assert_eq!(member.member_type, MemberType::Attribute);
    assert_eq!(member.classifier, "");
    assert!(student.methods.is_empty());
    assert!(student.annotations.is_empty());
    assert_eq!(student.css_classes, "default");
    assert!(student.styles.is_empty());
    assert_eq!(student.text, "Student");
    assert_eq!(student.type_name, "");

    assert_eq!(ast.relations.len(), 2);
    let first = &ast.relations[0];
    assert_eq!(first.id1, "Student");
    assert_eq!(first.id2, "IdCard");
    assert_eq!(first.relation.type1, Endpoint::None);
    assert_eq!(first.relation.type2, Endpoint::Aggregation);
    assert_eq!(first.relation.line_type, LineType::Line);
    assert_eq!(first.relation_title1, "1");
    assert_eq!(first.relation_title2, "1");
    assert_eq!(first.title.as_deref(), Some("carries"));
    let second = &ast.relations[1];
    assert_eq!(second.id1, "Student");
    assert_eq!(second.id2, "Bike");
    assert_eq!(second.title.as_deref(), Some("rides"));
}

#[test]
fn reverts_direction_to_default_once_direction_is_removed() {
    let ast = parsed("classDiagram\n          direction RL\n          class A");
    assert_eq!(ast.get_direction(), Direction::Rl);
    let ast = parsed("classDiagram\n          class B");
    assert_eq!(ast.get_direction(), Direction::Tb);
}

// -- classDiagram.spec.ts: "when parsing class defined in brackets" --

#[test]
fn handles_member_definitions() {
    parsed("classDiagram\nclass Car{\n+int wheels\n}");
}

#[test]
fn handles_method_definitions() {
    parsed("classDiagram\nclass Car{\n+size()\n}");
}

#[test]
fn handles_a_mix_of_members_defined_in_and_outside_of_brackets() {
    parsed("classDiagram\nclass Car{\n+int wheels\n}\nCar : +ArrayList size()\n");
}

#[test]
fn handles_member_and_method_definitions() {
    parsed("classDiagram\nclass Dummy_Class {\nString data\nvoid methods()\n}");
}

#[test]
fn handles_return_types_on_methods() {
    parsed(
        "classDiagram\nclass Flight {\nint flightNumber\ndatetime departureTime\ngetDepartureTime() datetime\n}",
    );
}

#[test]
fn adds_bracket_members_in_right_order() {
    let body = "classDiagram\nclass Class1 {\nint testMember\ntest()\nstring fooMember\nfoo()\n}";
    let ast = parsed(body);
    let class = ast.get_class("Class1").unwrap();
    assert_eq!(class.members.len(), 2);
    assert_eq!(class.methods.len(), 2);
    assert_eq!(
        class.members[0].display_details().display_text,
        "int testMember"
    );
    assert_eq!(
        class.members[1].display_details().display_text,
        "string fooMember"
    );
    assert_eq!(class.methods[0].display_details().display_text, "test()");
    assert_eq!(class.methods[1].display_details().display_text, "foo()");
}

#[test]
fn parses_a_class_with_a_text_label_and_members() {
    let ast = parsed("classDiagram\nclass C1[\"Class 1 with text label\"] {\n+member1\n}");
    let c1 = ast.get_class("C1").unwrap();
    assert_eq!(c1.label, "Class 1 with text label");
    assert_eq!(c1.members.len(), 1);
    assert_eq!(c1.members[0].display_details().display_text, "+member1");
}

#[test]
fn parses_a_class_with_a_text_label_members_and_annotation() {
    let ast =
        parsed("classDiagram\nclass C1[\"Class 1 with text label\"] {\n<<interface>>\n+member1\n}");
    let c1 = ast.get_class("C1").unwrap();
    assert_eq!(c1.label, "Class 1 with text label");
    assert_eq!(c1.members.len(), 1);
    assert_eq!(c1.members[0].display_details().display_text, "+member1");
    assert_eq!(c1.annotations.len(), 1);
    assert_eq!(c1.annotations[0], "interface");
}

// -- classDiagram.spec.ts: "when parsing comments" --

#[test]
fn handles_comments_at_the_start() {
    parsed(
        "%% Comment\n        classDiagram\n        class Class1 {\n          int : test\n          string : foo\n          test()\n          foo()\n        }",
    );
}

#[test]
fn handles_comments_at_the_end() {
    parsed(
        "classDiagram\nclass Class1 {\nint : test\nstring : foo\ntest()\nfoo()\n\n}\n%% Comment\n",
    );
}

#[test]
fn handles_comments_at_the_end_without_a_trailing_newline() {
    parsed("classDiagram\nclass Class1 {\nint : test\nstring : foo\ntest()\nfoo()\n}\n%% Comment");
}

#[test]
fn handles_a_comment_with_multiple_line_feeds() {
    parsed(
        "classDiagram\n\n\n%% Comment\n\nclass Class1 {\nint : test\nstring : foo\ntest()\nfoo()\n}",
    );
}

#[test]
fn handles_a_comment_with_mermaid_class_diagram_code_in_it() {
    parsed(
        "classDiagram\n%% Comment Class1 <|-- Class02\nclass Class1 {\nint : test\nstring : foo\ntest()\nfoo()\n}",
    );
}

#[test]
fn handles_a_comment_inside_brackets() {
    parsed(
        "classDiagram\nclass Class1 {\n%% Comment Class1 <|-- Class02\nint : test\nstring : foo\ntest()\nfoo()\n}",
    );
}

// -- classDiagram.spec.ts: "when parsing click statements" --

#[test]
fn handles_href_link() {
    let ast = parsed("classDiagram\nclass Class1 \nclick Class1 href \"google.com\" ");
    let class = ast.get_class("Class1").unwrap();
    assert_eq!(class.link.as_deref(), Some("google.com"));
    assert_eq!(class.css_classes, "default clickable");
}

#[test]
fn handles_href_link_with_tooltip() {
    let ast =
        parsed("classDiagram\nclass Class1 \nclick Class1 href \"google.com\" \"A Tooltip\" ");
    let class = ast.get_class("Class1").unwrap();
    assert_eq!(class.link.as_deref(), Some("google.com"));
    assert_eq!(class.tooltip.as_deref(), Some("A Tooltip"));
    assert_eq!(class.css_classes, "default clickable");
}

#[test]
fn handles_href_link_with_tooltip_and_target() {
    let ast = parsed(
        "classDiagram\nclass Class1\nClass1 : someMethod()\nclick Class1 href \"google.com\" \"A tooltip\" _self",
    );
    let class = ast.get_class("Class1").unwrap();
    assert_eq!(class.link.as_deref(), Some("google.com"));
    assert_eq!(class.tooltip.as_deref(), Some("A tooltip"));
    assert_eq!(class.link_target.as_deref(), Some("_self"));
    assert_eq!(class.css_classes, "default clickable");
}

#[test]
fn handles_function_call() {
    let ast = parsed("classDiagram\nclass Class1 \nclick Class1 call functionCall() ");
    let class = ast.get_class("Class1").unwrap();
    assert!(class.have_callback);
    assert_eq!(class.css_classes, "default clickable");
}

#[test]
fn handles_function_call_with_tooltip() {
    let ast =
        parsed("classDiagram\nclass Class1 \nclick Class1 call functionCall() \"A Tooltip\" ");
    let class = ast.get_class("Class1").unwrap();
    assert!(class.have_callback);
    assert_eq!(class.tooltip.as_deref(), Some("A Tooltip"));
}

#[test]
fn handles_function_call_with_arguments() {
    // The arguments only feed upstream's DOM handler, which this crate does not have.
    let ast = parsed(
        "classDiagram\nclass Class1\nClass1 : someMethod()\nclick Class1 call functionCall(test, test1, test2)",
    );
    assert!(ast.get_class("Class1").unwrap().have_callback);
}

#[test]
fn handles_function_call_with_arguments_and_tooltip() {
    let ast = parsed(
        "classDiagram\nclass Class1\nClass1 : someMethod()\nclick Class1 call functionCall(\"test0\", test1, test2) \"A Tooltip\"",
    );
    let class = ast.get_class("Class1").unwrap();
    assert!(class.have_callback);
    assert_eq!(class.tooltip.as_deref(), Some("A Tooltip"));
}

// -- classDiagram.spec.ts: "when parsing annotations" --

#[test]
fn handles_class_annotations() {
    let ast = parsed("classDiagram\nclass Class1\n<<interface>> Class1");
    let class = ast.get_class("Class1").unwrap();
    assert_eq!(class.annotations.len(), 1);
    assert!(class.members.is_empty());
    assert!(class.methods.is_empty());
    assert_eq!(class.annotations[0], "interface");
}

#[test]
fn handles_class_annotations_with_members_and_methods() {
    let ast = parsed(
        "classDiagram\nclass Class1\nClass1 : int test\nClass1 : test()\n<<interface>> Class1",
    );
    let class = ast.get_class("Class1").unwrap();
    assert_eq!(class.annotations.len(), 1);
    assert_eq!(class.members.len(), 1);
    assert_eq!(class.methods.len(), 1);
    assert_eq!(class.annotations[0], "interface");
}

#[test]
fn handles_class_annotations_in_brackets() {
    let ast = parsed("classDiagram\nclass Class1 {\n<<interface>>\n}");
    let class = ast.get_class("Class1").unwrap();
    assert_eq!(class.annotations.len(), 1);
    assert!(class.members.is_empty());
    assert!(class.methods.is_empty());
    assert_eq!(class.annotations[0], "interface");
}

#[test]
fn handles_class_annotations_in_brackets_with_members_and_methods() {
    let ast = parsed("classDiagram\nclass Class1 {\n<<interface>>\nint : test\ntest()\n}");
    let class = ast.get_class("Class1").unwrap();
    assert_eq!(class.annotations.len(), 1);
    assert_eq!(class.members.len(), 1);
    assert_eq!(class.methods.len(), 1);
    assert_eq!(class.annotations[0], "interface");
}

#[test]
fn handles_the_docs_example_separate_line_annotation_with_members() {
    let ast = parsed(
        "classDiagram\nclass Shape\n<<interface>> Shape\nShape : noOfVertices\nShape : draw()",
    );
    let shape = ast.get_class("Shape").unwrap();
    assert_eq!(shape.annotations.len(), 1);
    assert_eq!(shape.annotations[0], "interface");
    assert_eq!(shape.members.len(), 1);
    assert_eq!(shape.methods.len(), 1);
}

#[test]
fn handles_the_docs_example_nested_annotation_with_enumeration() {
    let body = "classDiagram\nclass Shape{\n    <<interface>>\n    noOfVertices\n    draw()\n}\nclass Color{\n    <<enumeration>>\n    RED\n    BLUE\n    GREEN\n    WHITE\n    BLACK\n}";
    let ast = parsed(body);
    assert_eq!(ast.get_class("Shape").unwrap().annotations[0], "interface");
    assert_eq!(
        ast.get_class("Color").unwrap().annotations[0],
        "enumeration"
    );
}

#[test]
fn handles_inline_annotation_syntax() {
    let ast = parsed("classDiagram\nclass Shape <<interface>>");
    let shape = ast.get_class("Shape").unwrap();
    assert_eq!(shape.annotations.len(), 1);
    assert_eq!(shape.annotations[0], "interface");
}

#[test]
fn handles_inline_annotation_with_members() {
    let ast = parsed("classDiagram\nclass Shape <<interface>> {\n    noOfVertices\n    draw()\n}");
    let shape = ast.get_class("Shape").unwrap();
    assert_eq!(shape.annotations.len(), 1);
    assert_eq!(shape.annotations[0], "interface");
    assert_eq!(shape.members.len(), 1);
    assert_eq!(shape.methods.len(), 1);
}

#[test]
fn handles_inline_annotation_with_empty_body() {
    let ast = parsed("classDiagram\nclass Shape <<interface>> {\n}");
    let shape = ast.get_class("Shape").unwrap();
    assert_eq!(shape.annotations.len(), 1);
    assert_eq!(shape.annotations[0], "interface");
}

// -- classDiagram.spec.ts: "given a class diagram with members and methods" --

#[test]
fn handles_simple_member_declaration() {
    parsed("classDiagram\nclass Car\nCar : wheels");
}

#[test]
fn handles_direct_member_declaration() {
    let ast = parsed("classDiagram\nCar : wheels");
    let car = ast.get_class("Car").unwrap();
    assert_eq!(car.members.len(), 1);
    assert_eq!(car.members[0].id, "wheels");
}

#[test]
fn handles_direct_member_declaration_with_type() {
    let ast = parsed("classDiagram\nCar : int wheels");
    let car = ast.get_class("Car").unwrap();
    assert_eq!(car.members.len(), 1);
    assert_eq!(car.members[0].id, "int wheels");
}

#[test]
fn handles_simple_member_declaration_with_type() {
    parsed("classDiagram\nclass Car\nCar : int wheels");
}

#[test]
fn handles_visibility() {
    let ast = parsed(
        "classDiagram\nclass actual\nactual : -int privateMember\nactual : +int publicMember\nactual : #int protectedMember\nactual : ~int privatePackage",
    );
    let actual = ast.get_class("actual").unwrap();
    assert_eq!(actual.members.len(), 4);
    assert!(actual.methods.is_empty());
    assert_eq!(
        actual.members[0].display_details().display_text,
        "-int privateMember"
    );
    assert_eq!(
        actual.members[1].display_details().display_text,
        "+int publicMember"
    );
    assert_eq!(
        actual.members[2].display_details().display_text,
        "#int protectedMember"
    );
    assert_eq!(
        actual.members[3].display_details().display_text,
        "~int privatePackage"
    );
}

#[test]
fn handles_generic_types() {
    parsed("classDiagram\nclass Car\nCar : -List~Wheel~ wheels");
}

#[test]
fn handles_method_definition() {
    parsed("classDiagram\nclass Car\nCar : GetSize()");
}

#[test]
fn handles_simple_return_types() {
    parsed("classDiagram\nclass Object\nObject : getObject() Object");
}

#[test]
fn handles_return_types_as_array() {
    parsed("classDiagram\nclass Object\nObject : getObjects() Object[]");
}

#[test]
fn handles_method_visibility() {
    parsed(
        "classDiagram\nclass actual\nactual : -privateMethod()\nactual : +publicMethod()\nactual : #protectedMethod()\n",
    );
}

#[test]
fn handles_abstract_methods() {
    let ast = parsed("classDiagram\nclass Class1\nClass1 : someMethod()*");
    let class = ast.get_class("Class1").unwrap();
    assert!(class.annotations.is_empty());
    assert!(class.members.is_empty());
    assert_eq!(class.methods.len(), 1);
    let method = &class.methods[0];
    assert_eq!(method.display_details().display_text, "someMethod()");
    assert_eq!(method.display_details().css_style, "font-style:italic;");
}

#[test]
fn handles_static_methods() {
    let ast = parsed("classDiagram\nclass Class1\nClass1 : someMethod()$");
    let class = ast.get_class("Class1").unwrap();
    assert!(class.annotations.is_empty());
    assert!(class.members.is_empty());
    assert_eq!(class.methods.len(), 1);
    let method = &class.methods[0];
    assert_eq!(method.display_details().display_text, "someMethod()");
    assert_eq!(
        method.display_details().css_style,
        "text-decoration:underline;"
    );
}

#[test]
fn handles_generic_types_in_arguments() {
    parsed("classDiagram\nclass Car\nCar : +setWheels(List~Wheel~ wheels)");
}

#[test]
fn handles_generic_return_types() {
    parsed("classDiagram\nclass Car\nCar : +getWheels() List~Wheel~");
}

#[test]
fn handles_generic_types_in_members_in_a_class_with_brackets() {
    parsed(
        "classDiagram\nclass Car {\nList~Wheel~ wheels\nsetWheels(List~Wheel~ wheels)\n+getWheels() List~Wheel~\n}",
    );
}

#[test]
fn handles_an_empty_class_body() {
    let ast = parsed("classDiagram\nclass EmptyClass {}");
    let class = ast.get_class("EmptyClass").unwrap();
    assert_eq!(class.label, "EmptyClass");
    assert!(class.members.is_empty());
    assert!(class.methods.is_empty());
}

// -- classDiagram.spec.ts: "given a class diagram with generics" --

#[test]
fn handles_generic_class() {
    parsed("classDiagram\nclass Car~T~");
}

#[test]
fn handles_generic_class_with_relationships() {
    parsed(
        "classDiagram\nclass Car~T~\nDriver -- Car : drives >\nCar *-- Wheel : have 4 >\nCar -- Person : < owns",
    );
}

#[test]
fn handles_generic_class_with_a_literal_name() {
    parsed(
        "classDiagram\nclass `Car`~T~\nDriver -- `Car` : drives >\n`Car` *-- Wheel : have 4 >\n`Car` -- Person : < owns",
    );
}

#[test]
fn handles_generic_class_with_brackets() {
    parsed(
        "classDiagram\nclass Dummy_Class~T~ {\nString data\nvoid methods()\n}\n\nclass Flight {\nInteger flightNumber\nDate departureTime\n}",
    );
}

#[test]
fn handles_generic_class_with_brackets_and_a_literal_name() {
    parsed(
        "classDiagram\nclass `Dummy_Class`~T~ {\nString data\n  void methods()\n}\n\nclass Flight {\n   flightNumber : Integer\n   departureTime : Date\n}",
    );
}

#[test]
fn handles_namespaces() {
    let body = "classDiagram
namespace Namespace1 { class Class1 }
namespace Namespace2 { class Class1
}
namespace Namespace3 {
class Class1 {
int : test
string : foo
test()
foo()
}
}
namespace Namespace4 {
class Class1 {
int : test
string : foo
test()
foo()
}
class Class2 {
int : test
string : foo
test()
foo()
}
}
";
    parsed(body);
}

#[test]
fn handles_namespace_with_generic_types() {
    parsed(
        "classDiagram\n\nnamespace space {\n    class Square~Shape~{\n        int id\n        List~int~ position\n        setPoints(List~int~ points)\n        getPoints() List~int~\n    }\n}",
    );
}

// -- classDiagram.spec.ts: "given a class diagram with relationships" --

#[test]
fn handles_all_basic_relationships() {
    parsed(
        "classDiagram\nClass1 <|-- Class02\nClass03 *-- Class04\nClass05 o-- Class06\nClass07 .. Class08\nClass09 -- Class1",
    );
}

#[test]
fn handles_backticked_class_name() {
    parsed(
        "classDiagram\n`Class1` <|-- Class02\nClass03 *-- Class04\nClass05 o-- Class06\nClass07 .. Class08\nClass09 -- Class1",
    );
}

#[test]
fn handles_generics_in_relationships() {
    let ast = parsed("classDiagram\nClass1~T~ <|-- Class02");
    assert_eq!(ast.get_class("Class1").unwrap().id, "Class1");
    assert_eq!(ast.get_class("Class1").unwrap().type_name, "T");
    assert_eq!(ast.get_class("Class02").unwrap().id, "Class02");
    assert_eq!(ast.relations[0].relation.type1, Endpoint::Extension);
    assert_eq!(ast.relations[0].relation.type2, Endpoint::None);
    assert_eq!(ast.relations[0].relation.line_type, LineType::Line);
}

#[test]
fn handles_relationships_with_labels() {
    parsed(
        "classDiagram\nclass Car\nDriver -- Car : drives >\nCar *-- Wheel : have 4 >\nCar -- Person : < owns",
    );
}

#[test]
fn handles_relation_definitions_extension() {
    let ast = parsed("classDiagram\nClass1 <|-- Class02");
    assert_eq!(ast.get_class("Class1").unwrap().id, "Class1");
    assert_eq!(ast.get_class("Class02").unwrap().id, "Class02");
    assert_eq!(ast.relations[0].relation.type1, Endpoint::Extension);
    assert_eq!(ast.relations[0].relation.type2, Endpoint::None);
    assert_eq!(ast.relations[0].relation.line_type, LineType::Line);
}

#[test]
fn handles_relation_definitions_of_different_types_and_directions() {
    parsed(
        "classDiagram\nClass11 <|.. Class12\nClass13 --> Class14\nClass15 ..> Class16\nClass17 ..|> Class18\nClass19 <--* Class20",
    );
}

#[test]
fn handles_cardinality_and_labels() {
    parsed(
        "classDiagram\nClass1 \"1\" *-- \"many\" Class02 : contains\nClass03 o-- Class04 : aggregation\nClass05 --> \"1\" Class06",
    );
}

#[test]
fn handles_dashed_relation_definitions_of_different_types_and_directions() {
    parsed(
        "classDiagram\nClass11 <|.. Class12\nClass13 <.. Class14\nClass15 ..|> Class16\nClass17 ..> Class18\nClass19 .. Class20",
    );
}

#[test]
fn handles_relation_definitions_aggregation_and_dotted_line() {
    let ast = parsed("classDiagram\nClass1 o.. Class02");
    assert_eq!(ast.get_class("Class1").unwrap().id, "Class1");
    assert_eq!(ast.get_class("Class02").unwrap().id, "Class02");
    assert_eq!(ast.relations[0].relation.type1, Endpoint::Aggregation);
    assert_eq!(ast.relations[0].relation.type2, Endpoint::None);
    assert_eq!(ast.relations[0].relation.line_type, LineType::DottedLine);
}

#[test]
fn handles_relation_definitions_composition_on_both_sides() {
    let ast = parsed("classDiagram\nClass1 *--* Class02");
    assert_eq!(ast.get_class("Class1").unwrap().id, "Class1");
    assert_eq!(ast.get_class("Class02").unwrap().id, "Class02");
    assert_eq!(ast.relations[0].relation.type1, Endpoint::Composition);
    assert_eq!(ast.relations[0].relation.type2, Endpoint::Composition);
    assert_eq!(ast.relations[0].relation.line_type, LineType::Line);
}

#[test]
fn handles_relation_definitions_with_no_types() {
    let ast = parsed("classDiagram\nClass1 -- Class02");
    assert_eq!(ast.get_class("Class1").unwrap().id, "Class1");
    assert_eq!(ast.get_class("Class02").unwrap().id, "Class02");
    assert_eq!(ast.relations[0].relation.type1, Endpoint::None);
    assert_eq!(ast.relations[0].relation.type2, Endpoint::None);
    assert_eq!(ast.relations[0].relation.line_type, LineType::Line);
}

#[test]
fn handles_relation_definitions_with_type_only_on_right_side() {
    let ast = parsed("classDiagram\nClass1 --|> Class02");
    assert_eq!(ast.get_class("Class1").unwrap().id, "Class1");
    assert_eq!(ast.get_class("Class02").unwrap().id, "Class02");
    assert_eq!(ast.relations[0].relation.type1, Endpoint::None);
    assert_eq!(ast.relations[0].relation.type2, Endpoint::Extension);
    assert_eq!(ast.relations[0].relation.line_type, LineType::Line);
}

#[test]
fn handles_multiple_classes_and_relation_definitions() {
    let body = "classDiagram\nClass1 <|-- Class02\nClass03 *-- Class04\nClass05 o-- Class06\nClass07 .. Class08\nClass09 -- Class10";
    let ast = parsed(body);
    assert_eq!(ast.get_class("Class1").unwrap().id, "Class1");
    assert_eq!(ast.get_class("Class10").unwrap().id, "Class10");
    assert_eq!(ast.relations.len(), 5);
    assert_eq!(ast.relations[0].relation.type1, Endpoint::Extension);
    assert_eq!(ast.relations[0].relation.type2, Endpoint::None);
    assert_eq!(ast.relations[0].relation.line_type, LineType::Line);
    assert_eq!(ast.relations[3].relation.type1, Endpoint::None);
    assert_eq!(ast.relations[3].relation.type2, Endpoint::None);
    assert_eq!(ast.relations[3].relation.line_type, LineType::DottedLine);
}

#[test]
fn handles_generic_class_with_relation_definitions() {
    let ast = parsed("classDiagram\nClass01~T~ <|-- Class02");
    assert_eq!(ast.get_class("Class01").unwrap().id, "Class01");
    assert_eq!(ast.get_class("Class01").unwrap().type_name, "T");
    assert_eq!(ast.get_class("Class02").unwrap().id, "Class02");
    assert_eq!(ast.relations[0].relation.type1, Endpoint::Extension);
    assert_eq!(ast.relations[0].relation.type2, Endpoint::None);
    assert_eq!(ast.relations[0].relation.line_type, LineType::Line);
}

#[test]
fn handles_links_and_css() {
    let ast =
        parsed("classDiagram\nclass Class1\nClass1 : someMethod()\nlink Class1 \"google.com\"");
    let class = ast.get_class("Class1").unwrap();
    assert_eq!(class.link.as_deref(), Some("google.com"));
    assert_eq!(class.css_classes, "default clickable");
}

#[test]
fn handles_click_and_href_link_and_css() {
    let ast = parsed(
        "classDiagram\nclass Class1\nClass1 : someMethod()\nclick Class1 href \"google.com\"",
    );
    let class = ast.get_class("Class1").unwrap();
    assert_eq!(class.link.as_deref(), Some("google.com"));
    assert_eq!(class.css_classes, "default clickable");
}

#[test]
fn handles_links_with_a_tooltip() {
    let ast = parsed(
        "classDiagram\nclass Class1\nClass1 : someMethod()\nlink Class1 \"google.com\" \"A tooltip\"",
    );
    let class = ast.get_class("Class1").unwrap();
    assert_eq!(class.link.as_deref(), Some("google.com"));
    assert_eq!(class.tooltip.as_deref(), Some("A tooltip"));
    assert_eq!(class.css_classes, "default clickable");
}

#[test]
fn handles_click_and_href_link_with_a_tooltip() {
    let ast = parsed(
        "classDiagram\nclass Class1\nClass1 : someMethod()\nclick Class1 href \"google.com\" \"A tooltip\"",
    );
    let class = ast.get_class("Class1").unwrap();
    assert_eq!(class.link.as_deref(), Some("google.com"));
    assert_eq!(class.tooltip.as_deref(), Some("A tooltip"));
    assert_eq!(class.css_classes, "default clickable");
}

#[test]
fn handles_click_and_href_link_with_a_tooltip_and_target() {
    let ast = parsed(
        "classDiagram\nclass Class1\nClass1 : someMethod()\nclick Class1 href \"google.com\" \"A tooltip\" _self",
    );
    let class = ast.get_class("Class1").unwrap();
    assert_eq!(class.link_target.as_deref(), Some("_self"));
    assert_eq!(class.tooltip.as_deref(), Some("A tooltip"));
}

#[test]
fn handles_click_and_href_link() {
    let ast = parsed(
        "classDiagram\nclass Class1\nClass1 : someMethod()\nclick Class1 href \"google.com\"",
    );
    assert_eq!(
        ast.get_class("Class1").unwrap().link.as_deref(),
        Some("google.com")
    );
}

#[test]
fn handles_click_and_href_link_with_a_target() {
    let ast = parsed(
        "classDiagram\nclass Class1\nClass1 : someMethod()\nclick Class1 href \"google.com\" _self",
    );
    let class = ast.get_class("Class1").unwrap();
    assert_eq!(class.link_target.as_deref(), Some("_self"));
}

#[test]
fn handles_a_link_with_a_tooltip_and_target() {
    let ast = parsed(
        "classDiagram\nclass Class1\nClass1 : someMethod()\nlink Class1 \"google.com\" \"A tooltip\" _self",
    );
    let class = ast.get_class("Class1").unwrap();
    assert_eq!(class.link_target.as_deref(), Some("_self"));
    assert_eq!(class.tooltip.as_deref(), Some("A tooltip"));
}

#[test]
fn handles_a_callback() {
    let ast = parsed(
        "classDiagram\nclass Class1\nClass1 : someMethod()\ncallback Class1 \"functionCall\"",
    );
    assert!(ast.get_class("Class1").unwrap().have_callback);
}

#[test]
fn handles_click_and_call_callback() {
    let ast = parsed(
        "classDiagram\nclass Class1\nClass1 : someMethod()\nclick Class1 call functionCall()",
    );
    assert!(ast.get_class("Class1").unwrap().have_callback);
}

#[test]
fn handles_a_callback_with_arguments() {
    let ast = parsed(
        "classDiagram\nclass Class1\nClass1 : someMethod()\nclick Class1 call functionCall(\"test0\", test1, test2)",
    );
    assert!(ast.get_class("Class1").unwrap().have_callback);
}

#[test]
fn handles_a_callback_with_a_tooltip() {
    let ast = parsed(
        "classDiagram\nclass Class1\nClass1 : someMethod()\nclick Class1 call functionCall() \"A tooltip\"",
    );
    let class = ast.get_class("Class1").unwrap();
    assert!(class.have_callback);
    assert_eq!(class.tooltip.as_deref(), Some("A tooltip"));
}

#[test]
fn adds_classes_to_namespaces() {
    let body = "classDiagram
namespace Namespace1 {
class Class1 {
int : test
string : foo
test()
foo()
}
class Class2
}";
    let ast = parsed(body);
    let namespace = ast.get_namespace("Namespace1").expect("Namespace1");
    assert_eq!(namespace.classes.len(), 2);
    assert!(namespace.children.is_empty());
    assert_eq!(ast.get_class("Class1").unwrap().id, "Class1");
    assert_eq!(ast.get_classes().len(), 2);
}

#[test]
fn adds_relations_between_classes_of_different_namespaces() {
    let body = "classDiagram
      A1 --> B1
      namespace A {
        class A1 {
          +foo : string
        }
        class A2 {
          +bar : int
        }
      }
      namespace B {
        class B1 {
          +foo : bool
        }
        class B2 {
          +bar : float
        }
      }
      A2 --> B2";
    let ast = parsed(body);
    let namespace_a = ast.get_namespace("A").expect("A");
    let namespace_b = ast.get_namespace("B").expect("B");
    assert_eq!(namespace_a.classes.len(), 2);
    assert_eq!(
        ast.get_class("A1").unwrap().members[0]
            .display_details()
            .display_text,
        "+foo : string"
    );
    assert_eq!(
        ast.get_class("A2").unwrap().members[0]
            .display_details()
            .display_text,
        "+bar : int"
    );
    assert_eq!(namespace_b.classes.len(), 2);
    assert_eq!(
        ast.get_class("B1").unwrap().members[0]
            .display_details()
            .display_text,
        "+foo : bool"
    );
    assert_eq!(
        ast.get_class("B2").unwrap().members[0]
            .display_details()
            .display_text,
        "+bar : float"
    );
    assert_eq!(ast.get_classes().len(), 4);
    assert_eq!(ast.get_class("A1").unwrap().parent.as_deref(), Some("A"));
    assert_eq!(ast.get_class("A2").unwrap().parent.as_deref(), Some("A"));
    assert_eq!(ast.get_class("B1").unwrap().parent.as_deref(), Some("B"));
    assert_eq!(ast.get_class("B2").unwrap().parent.as_deref(), Some("B"));
    assert_eq!(ast.relations[0].id1, "A1");
    assert_eq!(ast.relations[0].id2, "B1");
    assert_eq!(ast.relations[1].id1, "A2");
    assert_eq!(ast.relations[1].id2, "B2");
}

// -- classDiagram.spec.ts: "when parsing classDiagram with text labels" --

#[test]
fn parses_a_label_with_a_relation() {
    let ast = parsed("classDiagram\n  class C1[\"Class 1 with text label\"]\n  C1 -->  C2\n      ");
    assert_eq!(
        ast.get_class("C1").unwrap().label,
        "Class 1 with text label"
    );
    assert_eq!(ast.get_class("C2").unwrap().label, "C2");
}

#[test]
fn parses_two_labels_with_a_relation() {
    let ast = parsed(
        "classDiagram\n  class C1[\"Class 1 with text label\"]\n  class C2[\"Class 2 with chars @?\"]\n  C1 -->  C2\n      ",
    );
    assert_eq!(
        ast.get_class("C1").unwrap().label,
        "Class 1 with text label"
    );
    assert_eq!(ast.get_class("C2").unwrap().label, "Class 2 with chars @?");
}

#[test]
fn parses_a_label_and_members_with_a_relation() {
    let ast = parsed(
        "classDiagram\n  class C1[\"Class 1 with text label\"] {\n    +member1\n  }\n  C1 -->  C2\n      ",
    );
    let c1 = ast.get_class("C1").unwrap();
    assert_eq!(c1.label, "Class 1 with text label");
    assert_eq!(c1.members.len(), 1);
    assert_eq!(c1.members[0].display_details().display_text, "+member1");
    assert_eq!(ast.get_class("C2").unwrap().label, "C2");
}

#[test]
fn parses_a_label_members_and_annotation_with_a_relation() {
    let ast = parsed(
        "classDiagram\n  class C1[\"Class 1 with text label\"] {\n    <<interface>>\n    +member1\n  }\n  C1 -->  C2\n      ",
    );
    let c1 = ast.get_class("C1").unwrap();
    assert_eq!(c1.label, "Class 1 with text label");
    assert_eq!(c1.members.len(), 1);
    assert_eq!(c1.annotations.len(), 1);
    assert_eq!(c1.annotations[0], "interface");
    assert_eq!(c1.members[0].display_details().display_text, "+member1");
    assert_eq!(ast.get_class("C2").unwrap().label, "C2");
}

#[test]
fn parses_a_label_and_css_class_shorthand_with_a_relation() {
    let ast = parsed(
        "classDiagram\nclass C1[\"Class 1 with text label\"]:::styleClass {\n  +member1\n}\nC1 -->  C2\n  ",
    );
    let c1 = ast.get_class("C1").unwrap();
    assert_eq!(c1.label, "Class 1 with text label");
    assert_eq!(c1.css_classes, "default styleClass");
    assert_eq!(c1.members[0].display_details().display_text, "+member1");
}

#[test]
fn parses_a_label_and_css_class_with_a_relation() {
    let ast = parsed(
        "classDiagram\nclass C1[\"Class 1 with text label\"] {\n  +member1\n}\nC1 --> C2\ncssClass \"C1\" styleClass\n  ",
    );
    let c1 = ast.get_class("C1").unwrap();
    assert_eq!(c1.label, "Class 1 with text label");
    assert_eq!(c1.css_classes, "default styleClass");
    assert_eq!(c1.members[0].display_details().display_text, "+member1");
}

#[test]
fn parses_two_labels_and_css_classes_with_a_relation() {
    let ast = parsed(
        "classDiagram\nclass C1[\"Class 1 with text label\"] {\n  +member1\n}\nclass C2[\"Long long long long long long long long long long label\"]\nC1 --> C2\ncssClass \"C1,C2\" styleClass\n  ",
    );
    let c1 = ast.get_class("C1").unwrap();
    assert_eq!(c1.label, "Class 1 with text label");
    assert_eq!(c1.css_classes, "default styleClass");
    let c2 = ast.get_class("C2").unwrap();
    assert_eq!(
        c2.label,
        "Long long long long long long long long long long label"
    );
    assert_eq!(c2.css_classes, "default styleClass");
}

#[test]
fn parses_two_labels_and_css_class_shorthands_with_a_relation() {
    let ast = parsed(
        "classDiagram\nclass C1[\"Class 1 with text label\"]:::styleClass1 {\n  +member1\n}\nclass C2[\"Class 2 !@#$%^&*() label\"]:::styleClass2\nC1 --> C2\n  ",
    );
    let c1 = ast.get_class("C1").unwrap();
    assert_eq!(c1.label, "Class 1 with text label");
    assert_eq!(c1.css_classes, "default styleClass1");
    let c2 = ast.get_class("C2").unwrap();
    assert_eq!(c2.label, "Class 2 !@#$%^&*() label");
    assert_eq!(c2.css_classes, "default styleClass2");
}

#[test]
fn parses_multiple_labels_with_relations() {
    let ast = parsed(
        "classDiagram\nclass C1[\"Class with text label\"]\nclass C2[\"Class with text label\"]\nclass C3[\"Class with text label\"]\nC1 --> C2\nC3 ..> C2\n  ",
    );
    for id in ["C1", "C2", "C3"] {
        assert_eq!(ast.get_class(id).unwrap().label, "Class with text label");
    }
}

#[test]
fn parses_different_labels_with_relations() {
    let body = "classDiagram
class C1[\"OneWord\"]
class C2[\"With, Comma\"]
class C3[\"With (Brackets)\"]
class C4[\"With [Brackets]\"]
class C5[\"With {Brackets}\"]
class C6[\" \" ]
class C7[\"With 1 number\"]
class C8[\"With . period...\"]
class C9[\"With - dash\"]
class C10[\"With _ underscore\"]
class C11[\"With ' single quote\"]
class C12[\"With ~!@#$%^&*()_+=-/?\"]
class C13[\"With Città foreign language\"]
";
    let ast = parsed(body);
    assert_eq!(ast.get_class("C1").unwrap().label, "OneWord");
    assert_eq!(ast.get_class("C2").unwrap().label, "With, Comma");
    assert_eq!(ast.get_class("C3").unwrap().label, "With (Brackets)");
    assert_eq!(ast.get_class("C4").unwrap().label, "With [Brackets]");
    assert_eq!(ast.get_class("C5").unwrap().label, "With {Brackets}");
    assert_eq!(ast.get_class("C6").unwrap().label, " ");
    assert_eq!(ast.get_class("C7").unwrap().label, "With 1 number");
    assert_eq!(ast.get_class("C8").unwrap().label, "With . period...");
    assert_eq!(ast.get_class("C9").unwrap().label, "With - dash");
    assert_eq!(ast.get_class("C10").unwrap().label, "With _ underscore");
    assert_eq!(ast.get_class("C11").unwrap().label, "With ' single quote");
    assert_eq!(
        ast.get_class("C12").unwrap().label,
        "With ~!@#$%^&*()_+=-/?"
    );
    assert_eq!(
        ast.get_class("C13").unwrap().label,
        "With Città foreign language"
    );
}

// -- parser/class.spec.js --

#[test]
fn handles_prototype_properties_as_names() {
    for prop in ["__proto__", "constructor"] {
        let body = format!("classDiagram\nclass {prop}");
        parse(&body).unwrap_or_else(|error| panic!("class {prop}: {error}"));
        let body = format!("classDiagram\nnamespace {prop} {{\n\tclass A\n}}");
        parse(&body).unwrap_or_else(|error| panic!("namespace {prop}: {error}"));
    }
}

#[test]
fn handles_backtick_quoted_namespace_names() {
    parse("classDiagram\nnamespace `A::B` {\n\tclass `IPC::Sender`\n}").expect("backticked names");
}

// -- classDiagram-styles.spec.js --

#[test]
fn applies_a_css_class_directly() {
    let ast = parsed("classDiagram\nclass Class01:::exClass");
    assert_eq!(
        ast.get_class("Class01").unwrap().css_classes,
        "default exClass"
    );
}

#[test]
fn applies_a_css_class_directly_with_a_struct() {
    let ast = parsed(
        "classDiagram\nclass Class1:::exClass {\nint : test\nstring : foo\ntest()\nfoo()\n}",
    );
    assert_eq!(
        ast.get_class("Class1").unwrap().css_classes,
        "default exClass"
    );
}

#[test]
fn applies_a_css_class_to_a_class_with_relations() {
    let ast = parsed("classDiagram\nClass01 <|-- Class02\ncssClass \"Class01\" exClass");
    assert_eq!(
        ast.get_class("Class01").unwrap().css_classes,
        "default exClass"
    );
}

#[test]
fn applies_a_css_class_to_a_class() {
    let ast = parsed("classDiagram\nclass Class01\n cssClass \"Class01\" exClass");
    assert_eq!(
        ast.get_class("Class01").unwrap().css_classes,
        "default exClass"
    );
}

#[test]
fn applies_a_css_class_to_a_comma_separated_list_of_classes() {
    let ast = parsed(
        "classDiagram\nclass Class01\n class Class02\n cssClass \"Class01,Class02\" exClass",
    );
    assert_eq!(
        ast.get_class("Class01").unwrap().css_classes,
        "default exClass"
    );
    assert_eq!(
        ast.get_class("Class02").unwrap().css_classes,
        "default exClass"
    );
}

#[test]
fn applies_a_style_to_an_individual_node() {
    let ast = parsed(
        "classDiagram\nclass Class01\n class Class02\n style Class01 fill:#f9f,stroke:#333,stroke-width:4px",
    );
    let styles = &ast.get_class("Class01").unwrap().styles;
    assert_eq!(styles[0], "fill:#f9f");
    assert_eq!(styles[1], "stroke:#333");
    assert_eq!(styles[2], "stroke-width:4px");
}

#[test]
fn defines_and_assigns_a_class_inside_the_diagram() {
    let ast =
        parsed("classDiagram\nclass Class01\n cssClass \"Class01\" pink\n classDef pink fill:#f9f");
    assert_eq!(
        ast.get_class("Class01").unwrap().css_classes,
        "default pink"
    );
}

#[test]
fn defines_and_assigns_a_class_using_shorthand_inside_the_diagram() {
    let ast = parsed("classDiagram\nclass Class01:::pink\n classDef pink fill:#f9f");
    assert_eq!(
        ast.get_class("Class01").unwrap().css_classes,
        "default pink"
    );
}

#[test]
fn assigns_styles_from_a_class_defined_inside_the_diagram() {
    let ast = parsed(
        "classDiagram\nclass Class01:::pink\n classDef pink fill:#f9f,stroke:#333,stroke-width:6px",
    );
    assert_eq!(
        ast.get_class("Class01").unwrap().styles,
        ["fill:#f9f", "stroke:#333", "stroke-width:6px"]
    );
}

#[test]
fn assigns_multiple_classes_and_styles_from_classes_defined_inside_the_diagram() {
    let ast = parsed(
        "classDiagram\nclass Class01:::pink\n cssClass \"Class01\" bold\n classDef pink fill:#f9f\n classDef bold stroke:#333,stroke-width:6px",
    );
    assert_eq!(
        ast.get_class("Class01").unwrap().styles,
        ["fill:#f9f", "stroke:#333", "stroke-width:6px"]
    );
    assert_eq!(
        ast.get_class("Class01").unwrap().css_classes,
        "default pink bold"
    );
}

// -- classTypes.spec.ts: methods --

fn assert_method_cases(cases: &[(&str, &str, &str)]) {
    for (input, display_text, css_style) in cases {
        let details = display(input, MemberType::Method);
        assert_eq!(&details.display_text, display_text, "input: {input:?}");
        assert_eq!(&details.css_style, css_style, "input: {input:?}");
    }
}

const STATIC: &str = "text-decoration:underline;";
const ABSTRACT: &str = "font-style:italic;";

#[test]
fn parses_methods_without_parameters() {
    assert_method_cases(&[
        ("getTime()", "getTime()", ""),
        ("+getTime()", "+getTime()", ""),
        ("-getTime()", "-getTime()", ""),
        ("#getTime()", "#getTime()", ""),
        ("~getTime()", "~getTime()", ""),
        ("getTime()$", "getTime()", STATIC),
        ("getTime()*", "getTime()", ABSTRACT),
    ]);
}

#[test]
fn parses_methods_with_a_single_parameter_value() {
    assert_method_cases(&[
        ("getTime(int)", "getTime(int)", ""),
        ("+getTime(int)", "+getTime(int)", ""),
        ("-getTime(int)", "-getTime(int)", ""),
        ("#getTime(int)", "#getTime(int)", ""),
        ("~getTime(int)", "~getTime(int)", ""),
        ("getTime(int)$", "getTime(int)", STATIC),
        ("getTime(int)*", "getTime(int)", ABSTRACT),
    ]);
}

#[test]
fn parses_methods_with_a_single_parameter_type_and_name() {
    assert_method_cases(&[
        ("getTime(int count)", "getTime(int count)", ""),
        ("+getTime(int count)", "+getTime(int count)", ""),
        ("-getTime(int count)", "-getTime(int count)", ""),
        ("#getTime(int count)", "#getTime(int count)", ""),
        ("~getTime(int count)", "~getTime(int count)", ""),
        ("getTime(int count)$", "getTime(int count)", STATIC),
        ("getTime(int count)*", "getTime(int count)", ABSTRACT),
    ]);
}

#[test]
fn parses_methods_with_a_single_parameter_name_and_type() {
    assert_method_cases(&[
        ("getTime(count int)", "getTime(count int)", ""),
        ("+getTime(count int)", "+getTime(count int)", ""),
        ("-getTime(count int)", "-getTime(count int)", ""),
        ("#getTime(count int)", "#getTime(count int)", ""),
        ("~getTime(count int)", "~getTime(count int)", ""),
        ("getTime(count int)$", "getTime(count int)", STATIC),
        ("getTime(count int)*", "getTime(count int)", ABSTRACT),
    ]);
}

#[test]
fn parses_methods_with_multiple_parameters() {
    assert_method_cases(&[
        (
            "getTime(string text, int count)",
            "getTime(string text, int count)",
            "",
        ),
        (
            "+getTime(string text, int count)",
            "+getTime(string text, int count)",
            "",
        ),
        (
            "-getTime(string text, int count)",
            "-getTime(string text, int count)",
            "",
        ),
        (
            "#getTime(string text, int count)",
            "#getTime(string text, int count)",
            "",
        ),
        (
            "~getTime(string text, int count)",
            "~getTime(string text, int count)",
            "",
        ),
        (
            "getTime(string text, int count)$",
            "getTime(string text, int count)",
            STATIC,
        ),
        (
            "getTime(string text, int count)*",
            "getTime(string text, int count)",
            ABSTRACT,
        ),
    ]);
}

#[test]
fn parses_methods_with_a_return_type() {
    assert_method_cases(&[
        ("getTime() DateTime", "getTime() : DateTime", ""),
        ("+getTime() DateTime", "+getTime() : DateTime", ""),
        ("-getTime() DateTime", "-getTime() : DateTime", ""),
        ("#getTime() DateTime", "#getTime() : DateTime", ""),
        ("~getTime() DateTime", "~getTime() : DateTime", ""),
        ("getTime() DateTime$", "getTime() : DateTime", STATIC),
        ("getTime()  DateTime*", "getTime() : DateTime", ABSTRACT),
    ]);
}

#[test]
fn parses_methods_with_a_generic_parameter() {
    assert_method_cases(&[
        ("getTimes(List~T~)", "getTimes(List<T>)", ""),
        ("+getTimes(List~T~)", "+getTimes(List<T>)", ""),
        ("-getTimes(List~T~)", "-getTimes(List<T>)", ""),
        ("#getTimes(List~T~)", "#getTimes(List<T>)", ""),
        ("~getTimes(List~T~)", "~getTimes(List<T>)", ""),
        ("getTimes(List~T~)$", "getTimes(List<T>)", STATIC),
        ("getTimes(List~T~)*", "getTimes(List<T>)", ABSTRACT),
    ]);
}

#[test]
fn parses_methods_with_two_generic_parameters() {
    assert_method_cases(&[
        (
            "getTimes(List~T~, List~OT~)",
            "getTimes(List<T>, List<OT>)",
            "",
        ),
        (
            "+getTimes(List~T~, List~OT~)",
            "+getTimes(List<T>, List<OT>)",
            "",
        ),
        (
            "-getTimes(List~T~, List~OT~)",
            "-getTimes(List<T>, List<OT>)",
            "",
        ),
        (
            "#getTimes(List~T~, List~OT~)",
            "#getTimes(List<T>, List<OT>)",
            "",
        ),
        (
            "~getTimes(List~T~, List~OT~)",
            "~getTimes(List<T>, List<OT>)",
            "",
        ),
        (
            "getTimes(List~T~, List~OT~)$",
            "getTimes(List<T>, List<OT>)",
            STATIC,
        ),
        (
            "getTimes(List~T~, List~OT~)*",
            "getTimes(List<T>, List<OT>)",
            ABSTRACT,
        ),
    ]);
}

#[test]
fn parses_methods_with_a_nested_generic_parameter() {
    assert_method_cases(&[
        (
            "getTimetableList(List~List~T~~)",
            "getTimetableList(List<List<T>>)",
            "",
        ),
        (
            "+getTimetableList(List~List~T~~)",
            "+getTimetableList(List<List<T>>)",
            "",
        ),
        (
            "-getTimetableList(List~List~T~~)",
            "-getTimetableList(List<List<T>>)",
            "",
        ),
        (
            "#getTimetableList(List~List~T~~)",
            "#getTimetableList(List<List<T>>)",
            "",
        ),
        (
            "~getTimetableList(List~List~T~~)",
            "~getTimetableList(List<List<T>>)",
            "",
        ),
        (
            "getTimetableList(List~List~T~~)$",
            "getTimetableList(List<List<T>>)",
            STATIC,
        ),
        (
            "getTimetableList(List~List~T~~)*",
            "getTimetableList(List<List<T>>)",
            ABSTRACT,
        ),
    ]);
}

#[test]
fn parses_methods_with_a_composite_generic_parameter() {
    assert_method_cases(&[
        ("getTimes(List~K, V~)", "getTimes(List<K, V>)", ""),
        ("+getTimes(List~K, V~)", "+getTimes(List<K, V>)", ""),
        ("-getTimes(List~K, V~)", "-getTimes(List<K, V>)", ""),
        ("#getTimes(List~K, V~)", "#getTimes(List<K, V>)", ""),
        ("~getTimes(List~K, V~)", "~getTimes(List<K, V>)", ""),
        ("getTimes(List~K, V~)$", "getTimes(List<K, V>)", STATIC),
        ("getTimes(List~K, V~)*", "getTimes(List<K, V>)", ABSTRACT),
    ]);
}

#[test]
fn parses_methods_with_a_generic_return_type() {
    assert_method_cases(&[
        ("getTimes() List~T~", "getTimes() : List<T>", ""),
        ("+getTimes() List~T~", "+getTimes() : List<T>", ""),
        ("-getTimes() List~T~", "-getTimes() : List<T>", ""),
        ("#getTimes() List~T~", "#getTimes() : List<T>", ""),
        ("~getTimes() List~T~", "~getTimes() : List<T>", ""),
        ("getTimes() List~T~$", "getTimes() : List<T>", STATIC),
        ("getTimes() List~T~*", "getTimes() : List<T>", ABSTRACT),
    ]);
}

#[test]
fn parses_methods_with_a_nested_generic_return_type() {
    assert_method_cases(&[
        (
            "getTimetableList() List~List~T~~",
            "getTimetableList() : List<List<T>>",
            "",
        ),
        (
            "+getTimetableList() List~List~T~~",
            "+getTimetableList() : List<List<T>>",
            "",
        ),
        (
            "-getTimetableList() List~List~T~~",
            "-getTimetableList() : List<List<T>>",
            "",
        ),
        (
            "#getTimetableList() List~List~T~~",
            "#getTimetableList() : List<List<T>>",
            "",
        ),
        (
            "~getTimetableList() List~List~T~~",
            "~getTimetableList() : List<List<T>>",
            "",
        ),
        (
            "getTimetableList() List~List~T~~$",
            "getTimetableList() : List<List<T>>",
            STATIC,
        ),
        (
            "getTimetableList() List~List~T~~*",
            "getTimetableList() : List<List<T>>",
            ABSTRACT,
        ),
    ]);
}

#[test]
fn parses_uncategorized_members() {
    let details = display("std::map ~int,string~ pMap;", MemberType::Attribute);
    assert_eq!(details.display_text, "std::map <int,string> pMap;");

    let details = display(
        "getTime~T~(this T, int seconds)$ DateTime",
        MemberType::Method,
    );
    assert_eq!(
        details.display_text,
        "getTime<T>(this T, int seconds) : DateTime"
    );
    assert_eq!(details.css_style, STATIC);
}

// -- classTypes.spec.ts: attributes --

#[test]
fn parses_attributes_without_modifiers() {
    let details = display("name String", MemberType::Attribute);
    assert_eq!(details.display_text, "name String");
    assert_eq!(details.css_style, "");
}

#[test]
fn parses_attributes_with_a_public_modifier() {
    let details = display("+name String", MemberType::Attribute);
    assert_eq!(details.display_text, "+name String");
    assert_eq!(details.css_style, "");
}

#[test]
fn parses_attributes_with_a_protected_modifier() {
    let details = display("#name String", MemberType::Attribute);
    assert_eq!(details.display_text, "#name String");
    assert_eq!(details.css_style, "");
}

#[test]
fn parses_attributes_with_a_private_modifier() {
    let details = display("-name String", MemberType::Attribute);
    assert_eq!(details.display_text, "-name String");
    assert_eq!(details.css_style, "");
}

#[test]
fn parses_attributes_with_an_internal_modifier() {
    let details = display("~name String", MemberType::Attribute);
    assert_eq!(details.display_text, "~name String");
    assert_eq!(details.css_style, "");
}

#[test]
fn parses_attributes_with_a_static_modifier() {
    let details = display("name String$", MemberType::Attribute);
    assert_eq!(details.display_text, "name String");
    assert_eq!(details.css_style, STATIC);
}

#[test]
fn parses_attributes_with_an_abstract_modifier() {
    let details = display("name String*", MemberType::Attribute);
    assert_eq!(details.display_text, "name String");
    assert_eq!(details.css_style, ABSTRACT);
}

// -- Errors carry a span --

#[test]
fn errors_carry_the_line_and_column_of_the_failing_token() {
    let error = parse("classDiagram\n  note foo").expect_err("a note needs a string");
    assert!(
        error.message.contains("Expecting 'STR'"),
        "{}",
        error.message
    );
    let span = error.span.expect("a span");
    assert_eq!(span.line, 2);
    assert_eq!(span.column, 8);
}

#[test]
fn unrecognized_text_errors_carry_a_span() {
    let error = parse("classDiagram\nclass A {\n}\n}").expect_err("a stray closing brace");
    assert!(
        error.message.contains("Unrecognized text"),
        "{}",
        error.message
    );
    let span = error.span.expect("a span");
    assert_eq!(span.line, 4);
    assert_eq!(span.column, 1);
}

// -- Grammar corners pinned against the Jison parser --

#[test]
fn errors_when_a_css_shorthand_body_is_empty() {
    // `classIdentifier STYLE_SEPARATOR alphaNumToken STRUCT_START members STRUCT_STOP`
    // has no empty variant, unlike a plain or annotated class body.
    assert!(parse("classDiagram\nclass A[\"x\"]:::y {\n}").is_err());
    assert!(parse("classDiagram\nclass A:::y {\n}").is_err());
    // A whitespace-only line is still a MEMBER, so the body is not empty.
    parsed("classDiagram\nclass A:::y {\n   \n}");
}

#[test]
fn errors_on_unterminated_text_at_the_end() {
    for body in [
        "classDiagram\nnote \"x",
        "classDiagram\nclass A~",
        "classDiagram\nclick A call f(",
        "classDiagram\naccTitle: ",
    ] {
        assert!(parse(body).is_err(), "{body:?}");
    }
}

#[test]
fn acc_title_takes_the_next_line_after_the_colon() {
    // `accTitle\s*":"\s*` lets `\s*` swallow the newline.
    let ast = parsed("classDiagram\naccTitle:\nclass A");
    assert_eq!(ast.acc_title.as_deref(), Some("class A"));
}

// -- Database details the upstream specs leave implicit --

#[test]
fn turns_a_lollipop_relation_into_an_interface_node() {
    let ast = parsed("classDiagram\nA ()-- B");
    assert_eq!(ast.interfaces.len(), 1);
    let interface = &ast.interfaces[0];
    assert_eq!(interface.id, "interface0");
    assert_eq!(interface.label, "A");
    assert_eq!(interface.class_id, "B");
    assert_eq!(ast.relations[0].id1, "interface0");
    assert_eq!(ast.relations[0].id2, "B");
    assert!(ast.get_class("A").is_none());
    assert!(ast.get_class("B").is_some());

    let ast = parsed("classDiagram\nA --() B");
    assert_eq!(ast.interfaces[0].label, "B");
    assert_eq!(ast.interfaces[0].class_id, "A");
    assert_eq!(ast.relations[0].id2, "interface0");
    assert!(ast.get_class("A").is_some());
    assert!(ast.get_class("B").is_none());
}

#[test]
fn class_definitions_keep_styles_and_text_styles() {
    let ast = parsed("classDiagram\nclass A:::pink\nclassDef pink fill:#f9f,color:#fff");
    let style_class = ast.style_classes.get("pink").expect("pink");
    assert_eq!(style_class.styles, ["fill:#f9f,color:#fff"]);
    assert_eq!(style_class.text_styles, ["bgFill:#f9f,color:#fff"]);
}

#[test]
fn assigns_notes_to_a_namespace() {
    let ast = parsed("classDiagram\nnamespace N {\nnote \"x\"\n}");
    assert_eq!(ast.get_namespace("N").unwrap().notes, ["note0"]);
    assert_eq!(ast.get_note(0).unwrap().parent.as_deref(), Some("N"));
}
