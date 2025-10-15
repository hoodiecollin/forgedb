use forgedb::ast::*;
use forgedb::parser::Parser;
use forgedb::{ComponentPropsGenerator, ComponentStubGenerator, RouteHandlerGenerator, StubTemplate};

#[test]
fn test_component_integration_full_workflow() {
    // Step 1: Parse a schema with component fields
    let schema_input = r#"
User {
  id: +uuid
  name: string
  email: string @email
  posts: [Post]
  card: tsx://components/user/UserCard @relations(*)
  profile: jsx://components/user/Profile @relations(posts)
  verify: api://routes/user/verify
}

Post {
  id: +uuid
  title: string
  content: string
  author: *User
}
"#;

    let mut parser = Parser::new(schema_input).expect("Failed to create parser");
    let schema = parser.parse().expect("Failed to parse schema");

    // Step 2: Verify schema was parsed correctly
    assert_eq!(schema.models.len(), 2);

    let user_model = &schema.models[0];
    assert_eq!(user_model.name, "User");
    assert_eq!(user_model.fields.len(), 7);

    // Verify TSX component field with @relations(*)
    let card_field = user_model.fields.iter().find(|f| f.name == "card").expect("card field not found");
    if let FieldType::Component(comp_ref) = &card_field.field_type {
        assert_eq!(comp_ref.protocol, ComponentProtocol::Tsx);
        assert_eq!(comp_ref.path, "components/user/UserCard");
        assert_eq!(comp_ref.relations, RelationInclusion::All);
    } else {
        panic!("card field should be Component type");
    }

    // Verify JSX component field with specific relations
    let profile_field = user_model.fields.iter().find(|f| f.name == "profile").expect("profile field not found");
    if let FieldType::Component(comp_ref) = &profile_field.field_type {
        assert_eq!(comp_ref.protocol, ComponentProtocol::Jsx);
        assert_eq!(comp_ref.path, "components/user/Profile");
        assert_eq!(comp_ref.relations, RelationInclusion::Specific(vec!["posts".to_string()]));
    } else {
        panic!("profile field should be Component type");
    }

    // Verify API component field
    let verify_field = user_model.fields.iter().find(|f| f.name == "verify").expect("verify field not found");
    if let FieldType::Component(comp_ref) = &verify_field.field_type {
        assert_eq!(comp_ref.protocol, ComponentProtocol::Api);
        assert_eq!(comp_ref.path, "routes/user/verify");
    } else {
        panic!("verify field should be Component type");
    }

    // Step 3: Generate TypeScript component props
    let props_generator = ComponentPropsGenerator::new();
    let props_content = props_generator.generate_props_types(&schema);

    // Verify props types are generated
    assert!(props_content.contains("UserCardProps"));
    assert!(props_content.contains("UserProfileProps"));
    assert!(props_content.contains("data: User"));

    // Step 4: Generate component stubs
    let stub_generator = ComponentStubGenerator::new();
    let stub_files = stub_generator.generate_stubs(&schema, StubTemplate::Detailed);

    // Should generate 2 component stubs (card and profile, not the API route)
    assert_eq!(stub_files.len(), 2);

    // Verify the TSX component stub
    let card_stub = stub_files.iter().find(|f| f.path.contains("UserCard")).expect("UserCard stub not found");
    assert!(card_stub.content.contains("UserCardProps"));
    assert!(card_stub.content.contains("export default function"));
    assert!(card_stub.path.ends_with("page.tsx"));

    // Verify the JSX component stub
    let profile_stub = stub_files.iter().find(|f| f.path.contains("Profile")).expect("Profile stub not found");
    assert!(profile_stub.content.contains("UserProfileProps"));
    assert!(profile_stub.path.ends_with("page.tsx"));

    // Step 5: Generate API route handlers
    let route_generator = RouteHandlerGenerator::new();
    let route_files = route_generator.generate_handlers(&schema);

    // Should generate 1 API route handler
    assert_eq!(route_files.len(), 1);

    let verify_route = &route_files[0];
    assert!(verify_route.path.contains("routes/user/verify"));
    assert!(verify_route.content.contains("NextRequest"));
    assert!(verify_route.content.contains("NextResponse"));
    assert!(verify_route.content.contains("export async function POST"));
}

#[test]
fn test_component_without_relations() {
    let schema_input = "Product {
  id: +uuid
  name: string
  thumbnail: tsx://components/ProductThumbnail
}";

    let mut parser = Parser::new(schema_input).expect("Failed to create parser");
    let schema = parser.parse().expect("Failed to parse schema");

    let product_model = &schema.models[0];
    let thumbnail_field = product_model.fields.iter().find(|f| f.name == "thumbnail").expect("thumbnail field not found");

    if let FieldType::Component(comp_ref) = &thumbnail_field.field_type {
        assert_eq!(comp_ref.relations, RelationInclusion::None);
    } else {
        panic!("thumbnail field should be Component type");
    }

    // Generate props - should not include relations
    let props_generator = ComponentPropsGenerator::new();
    let props_content = props_generator.generate_props_types(&schema);
    assert!(props_content.contains("ProductThumbnailProps"));
    assert!(props_content.contains("data: Product"));
}

#[test]
fn test_multiple_api_routes() {
    let schema_input = "Order {
  id: +uuid
  total: f64
  create: api://routes/order/create
  cancel: api://routes/order/cancel
  refund: api://routes/order/refund
}";

    let mut parser = Parser::new(schema_input).expect("Failed to create parser");
    let schema = parser.parse().expect("Failed to parse schema");

    let route_generator = RouteHandlerGenerator::new();
    let route_files = route_generator.generate_handlers(&schema);

    // Should generate 3 API route handlers
    assert_eq!(route_files.len(), 3);

    let paths: Vec<&str> = route_files.iter().map(|f| f.path.as_str()).collect();
    assert!(paths.iter().any(|p| p.contains("order/create")));
    assert!(paths.iter().any(|p| p.contains("order/cancel")));
    assert!(paths.iter().any(|p| p.contains("order/refund")));
}
