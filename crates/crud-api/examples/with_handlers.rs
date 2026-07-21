//! Intermediate example for forgedb-crud-api
//!
//! This example demonstrates error handling and ListResponse formatting
//! for CRUD operations.

use forgedb_crud_api::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

// Define a Product model
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Product {
    id: Uuid,
    name: String,
    description: String,
    price: f64,
    in_stock: bool,
}

#[derive(Debug, Deserialize)]
struct CreateProduct {
    name: String,
    description: String,
    price: f64,
    in_stock: bool,
}

#[derive(Debug, Deserialize)]
struct UpdateProduct {
    name: Option<String>,
    description: Option<String>,
    price: Option<f64>,
    in_stock: Option<bool>,
}

// Simple product storage
struct ProductStorage {
    products: HashMap<Uuid, Product>,
}

impl ProductStorage {
    fn new() -> Self {
        Self {
            products: HashMap::new(),
        }
    }
}

impl CrudOperations for ProductStorage {
    type Model = Product;
    type CreateInput = CreateProduct;
    type UpdateInput = UpdateProduct;

    fn list(&self) -> CrudResult<Vec<Self::Model>> {
        Ok(self.products.values().cloned().collect())
    }

    fn get(&self, id: &Uuid) -> CrudResult<Option<Self::Model>> {
        Ok(self.products.get(id).cloned())
    }

    fn create(&mut self, input: Self::CreateInput) -> CrudResult<Self::Model> {
        // Validate input
        if input.price < 0.0 {
            return Err(CrudError::ValidationError(
                "Price cannot be negative".to_string(),
            ));
        }
        
        if input.name.is_empty() {
            return Err(CrudError::ValidationError(
                "Name cannot be empty".to_string(),
            ));
        }

        let product = Product {
            id: Uuid::new_v4(),
            name: input.name,
            description: input.description,
            price: input.price,
            in_stock: input.in_stock,
        };

        self.products.insert(product.id, product.clone());
        Ok(product)
    }

    fn update(&mut self, id: &Uuid, input: Self::UpdateInput) -> CrudResult<Option<Self::Model>> {
        if let Some(product) = self.products.get_mut(id) {
            if let Some(name) = input.name {
                if name.is_empty() {
                    return Err(CrudError::ValidationError(
                        "Name cannot be empty".to_string(),
                    ));
                }
                product.name = name;
            }
            if let Some(description) = input.description {
                product.description = description;
            }
            if let Some(price) = input.price {
                if price < 0.0 {
                    return Err(CrudError::ValidationError(
                        "Price cannot be negative".to_string(),
                    ));
                }
                product.price = price;
            }
            if let Some(in_stock) = input.in_stock {
                product.in_stock = in_stock;
            }
            Ok(Some(product.clone()))
        } else {
            Ok(None)
        }
    }

    fn delete(&mut self, id: &Uuid) -> CrudResult<bool> {
        Ok(self.products.remove(id).is_some())
    }
}

fn main() {
    println!("=== ForgeDB CRUD API - With Error Handling ===\n");

    // Create storage
    let mut storage = ProductStorage::new();
    println!("✓ Created product storage\n");

    // Demonstrate list operation with ListResponse
    println!("--- List Products (Empty) ---");
    match storage.list() {
        Ok(products) => {
            let response = ListResponse::new(products);
            println!("Found {} products", response.total);
        }
        Err(e) => println!("Error: {:?}", e),
    }
    println!();

    // Create products
    println!("--- Creating Products ---");
    
    let product1_input = CreateProduct {
        name: "Laptop".to_string(),
        description: "High-performance laptop".to_string(),
        price: 999.99,
        in_stock: true,
    };

    match storage.create(product1_input) {
        Ok(product) => {
            println!("✓ Created: {} (${:.2}) - ID: {}", product.name, product.price, product.id);
        }
        Err(e) => println!("Error: {}", e),
    }

    let product2_input = CreateProduct {
        name: "Mouse".to_string(),
        description: "Wireless gaming mouse".to_string(),
        price: 49.99,
        in_stock: true,
    };

    match storage.create(product2_input) {
        Ok(product) => {
            println!("✓ Created: {} (${:.2}) - ID: {}", product.name, product.price, product.id);
        }
        Err(e) => println!("Error: {}", e),
    }
    println!();

    // Try to create an invalid product (negative price)
    println!("--- Attempting Invalid Create (negative price) ---");
    let invalid_input = CreateProduct {
        name: "Invalid Product".to_string(),
        description: "This should fail".to_string(),
        price: -10.0,
        in_stock: true,
    };

    match storage.create(invalid_input) {
        Ok(_) => println!("Unexpected success"),
        Err(e) => println!("✓ Expected error: {}", e),
    }
    println!();

    // Try to create with empty name
    println!("--- Attempting Invalid Create (empty name) ---");
    let invalid_input2 = CreateProduct {
        name: "".to_string(),
        description: "This should also fail".to_string(),
        price: 10.0,
        in_stock: true,
    };

    match storage.create(invalid_input2) {
        Ok(_) => println!("Unexpected success"),
        Err(e) => println!("✓ Expected error: {}", e),
    }
    println!();

    // List all products with pagination info
    println!("--- List All Products ---");
    match storage.list() {
        Ok(products) => {
            let response = ListResponse::with_pagination(
                products.clone(),
                products.len(),
                10, // limit
                0,  // offset
            );
            
            println!("Response with pagination:");
            println!("  Total: {}", response.total);
            println!("  Limit: {:?}", response.limit);
            println!("  Offset: {:?}", response.offset);
            println!("  Products:");
            for product in &response.data {
                println!(
                    "    - {} (${:.2}) - {} - ID: {}",
                    product.name,
                    product.price,
                    if product.in_stock { "In Stock" } else { "Out of Stock" },
                    product.id
                );
            }
        }
        Err(e) => println!("Error: {}", e),
    }

    println!("\n✓ Example completed successfully!");
}
