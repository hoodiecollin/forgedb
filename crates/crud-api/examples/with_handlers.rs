//! Intermediate example for forgedb-crud-api
//!
//! This example demonstrates using CrudHandlers to create
//! a type-safe API handler with proper error responses.

use forgedb_crud_api::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
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

// Thread-safe product storage
#[derive(Clone)]
struct ProductStorage {
    products: Arc<RwLock<HashMap<Uuid, Product>>>,
}

impl ProductStorage {
    fn new() -> Self {
        Self {
            products: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl CrudOperations for ProductStorage {
    type Model = Product;
    type CreateInput = CreateProduct;
    type UpdateInput = UpdateProduct;

    fn list(&self) -> CrudResult<Vec<Self::Model>> {
        let products = self.products.read().map_err(|e| {
            CrudError::InternalError(format!("Failed to acquire read lock: {}", e))
        })?;
        Ok(products.values().cloned().collect())
    }

    fn get(&self, id: &Uuid) -> CrudResult<Option<Self::Model>> {
        let products = self.products.read().map_err(|e| {
            CrudError::InternalError(format!("Failed to acquire read lock: {}", e))
        })?;
        Ok(products.get(id).cloned())
    }

    fn create(&mut self, input: Self::CreateInput) -> CrudResult<Self::Model> {
        // Validate input
        if input.price < 0.0 {
            return Err(CrudError::ValidationError(
                "Price cannot be negative".to_string(),
            ));
        }

        let product = Product {
            id: Uuid::new_v4(),
            name: input.name,
            description: input.description,
            price: input.price,
            in_stock: input.in_stock,
        };

        let mut products = self.products.write().map_err(|e| {
            CrudError::InternalError(format!("Failed to acquire write lock: {}", e))
        })?;
        
        products.insert(product.id, product.clone());
        Ok(product)
    }

    fn update(&mut self, id: &Uuid, input: Self::UpdateInput) -> CrudResult<Option<Self::Model>> {
        let mut products = self.products.write().map_err(|e| {
            CrudError::InternalError(format!("Failed to acquire write lock: {}", e))
        })?;

        if let Some(product) = products.get_mut(id) {
            if let Some(name) = input.name {
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
        let mut products = self.products.write().map_err(|e| {
            CrudError::InternalError(format!("Failed to acquire write lock: {}", e))
        })?;
        Ok(products.remove(id).is_some())
    }
}

fn main() {
    println!("=== ForgeDB CRUD API - With Handlers ===\n");

    // Create storage
    let mut storage = ProductStorage::new();
    println!("✓ Created product storage with thread-safe access\n");

    // Create handlers (wraps CRUD operations with error handling)
    let handlers = CrudHandlers::new();
    println!("✓ Created CRUD handlers\n");

    // Demonstrate list operation
    println!("--- List Products (Empty) ---");
    match handlers.list(&storage) {
        Ok(ListResponse { items, total }) => {
            println!("Found {} products", total);
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

    match handlers.create(&mut storage, product1_input) {
        Ok(product) => {
            println!("✓ Created: {} (${:.2}) - ID: {}", product.name, product.price, product.id);
        }
        Err(e) => println!("Error: {:?}", e),
    }

    let product2_input = CreateProduct {
        name: "Mouse".to_string(),
        description: "Wireless gaming mouse".to_string(),
        price: 49.99,
        in_stock: true,
    };

    match handlers.create(&mut storage, product2_input) {
        Ok(product) => {
            println!("✓ Created: {} (${:.2}) - ID: {}", product.name, product.price, product.id);
        }
        Err(e) => println!("Error: {:?}", e),
    }
    println!();

    // Try to create an invalid product (negative price)
    println!("--- Attempting Invalid Create ---");
    let invalid_input = CreateProduct {
        name: "Invalid Product".to_string(),
        description: "This should fail".to_string(),
        price: -10.0,
        in_stock: true,
    };

    match handlers.create(&mut storage, invalid_input) {
        Ok(_) => println!("Unexpected success"),
        Err(e) => println!("✓ Expected error: {:?}", e),
    }
    println!();

    // List all products
    println!("--- List All Products ---");
    match handlers.list(&storage) {
        Ok(ListResponse { items, total }) => {
            println!("Found {} products:", total);
            for product in items {
                println!(
                    "  - {} (${:.2}) - {} - ID: {}",
                    product.name,
                    product.price,
                    if product.in_stock { "In Stock" } else { "Out of Stock" },
                    product.id
                );
            }
        }
        Err(e) => println!("Error: {:?}", e),
    }

    println!("\n✓ Example completed successfully!");
}
