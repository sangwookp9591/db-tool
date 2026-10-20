use dbcore::*;
use driver_mongodb::MongoDriver;
use mongodb::{
    bson::{doc, oid::ObjectId, Bson, Document},
    Client,
};
use std::collections::BTreeMap;

#[tokio::test]
async fn real_mongodb_schema_graph() {
    let Ok(uri) = std::env::var("AXDB_TEST_MONGO") else {
        eprintln!("AXDB_TEST_MONGO unset: schema integration test not run");
        return;
    };
    assert_eq!(
        uri,
        "mongodb://axdb:testmongo@127.0.0.1:57017/?authSource=admin"
    );
    let test_database=format!("axdbt_d10_schema_{}",mongodb::bson::oid::ObjectId::new().to_hex());
    let client = Client::with_uri_str(uri).await.unwrap();
    let db = client.database(test_database.as_str());
    let prefix = format!("axdbt_{}_", ObjectId::new().to_hex());
    let users = format!("{prefix}users");
    let orders = format!("{prefix}orders");
    let view = format!("{prefix}lookup");
    let empty = format!("{prefix}empty");
    let sampled = format!("{prefix}sampled");
    let user_field = format!("{prefix}userId");
    let user_snake = format!("{prefix}user_id");
    let missing_field = format!("{prefix}emptyId");
    let id = ObjectId::new();
    db.collection::<Document>(&users)
        .insert_many([
            doc! {"_id":id,"profile":{"name":"Alice"},"optional":true},
            doc! {"profile":{"name":Bson::Null}},
        ])
        .await
        .unwrap();
    db.create_collection(&empty).await.unwrap();
    db.collection::<Document>(&sampled)
        .insert_many((0..201).map(|i| {
            let mut document = doc! {"_id":i};
            document.insert(format!("observed_{i}"), true);
            document
        }))
        .await
        .unwrap();
    db.collection::<Document>(&orders)
        .insert_one(doc! {
            "userId":id, &user_field:id, &user_snake:id, &missing_field:ObjectId::new(),
            "ref":{"$ref":&users,"$id":id,"$db":test_database.as_str()},
            "cross":{"$ref":&users,"$id":id,"$db":"other_database"},
            "items":[{"price":1},{"price":"varies"}],
        })
        .await
        .unwrap();
    db.create_collection(&view)
        .view_on(orders.clone())
        .pipeline(vec![doc! {
            "$lookup":{"from":&users,"localField":"userId","foreignField":"_id","as":"user"}
        }])
        .await
        .unwrap();
    let cfg = ConnectConfig {
        fields: BTreeMap::from([
            ("uri".into(), "mongodb://127.0.0.1:57017".into()),
            ("database".into(), test_database.as_str().into()),
            ("user".into(), "axdb".into()),
        ]),
        secret: Some("testmongo".into()),
        connect_timeout_ms: 3000,
    };
    let driver = MongoDriver::new();
    assert!(driver.manifest().capabilities.schema_graph);
    let session = driver.connect(&cfg).await.unwrap();
    let roots = session.list_children(None).await.unwrap();
    let database = roots.iter().find(|n| n.label == test_database.as_str()).unwrap();
    let action = database.actions.iter().find(|a| a.id == "diagram").unwrap();
    assert_eq!(
        action.kind,
        NodeActionKind::OpenSchemaGraph {
            scope: Some(database.id.clone())
        }
    );
    let graph = session.schema_graph(Some(&database.id)).await.unwrap();
    let entity = |name: &str| graph.entities.iter().find(|e| e.name == name).unwrap();
    let user = entity(&users);
    assert_eq!(user.kind, "collection");
    let name = user
        .fields
        .iter()
        .find(|f| f.name == "profile.name")
        .unwrap();
    assert_eq!(name.type_name, "null|string");
    assert!(name.nullable);
    assert!(
        user.fields
            .iter()
            .find(|f| f.name == "optional")
            .unwrap()
            .nullable
    );
    assert!(user.fields.iter().find(|f| f.name == "_id").unwrap().pk);
    assert!(entity(&empty).fields.is_empty());
    assert_eq!(
        entity(&sampled).fields.len(),
        201,
        "200 unique sampled fields plus _id"
    );
    let order = entity(&orders);
    assert_eq!(
        order
            .fields
            .iter()
            .find(|f| f.name == "items.price")
            .unwrap()
            .type_name,
        "int32|string"
    );
    assert!(order.fields.iter().any(|f| f.name == "ref.$db"));
    for field in [&user_field, &user_snake, "ref.$id"] {
        assert!(
            graph.relations.iter().any(|r| r.from == order.id
                && r.to == user.id
                && r.kind == "inferred_reference"
                && r.from_fields == [field]
                && r.to_fields == ["_id"]),
            "missing relation {field}"
        );
    }
    assert!(!graph.relations.iter().any(|r| r.from == order.id
        && (r.from_fields == [missing_field.as_str()] || r.from_fields == ["cross.$id"])));
    let lookup = entity(&view);
    assert_eq!(lookup.kind, "view");
    assert!(graph.relations.iter().any(|r| r.from == lookup.id
        && r.to == user.id
        && r.kind == "view_lookup"
        && r.from_fields == ["userId"]
        && r.to_fields == ["_id"]));
    for scope in [None, Some(order.id.as_str())] {
        let scoped = session.schema_graph(scope).await.unwrap();
        assert_eq!(
            scoped.entities.iter().map(|e| &e.id).collect::<Vec<_>>(),
            graph.entities.iter().map(|e| &e.id).collect::<Vec<_>>()
        );
        assert_eq!(scoped.relations, graph.relations);
    }
    let result = session
        .browse(
            &lookup.id,
            PageRequest {
                limit: 10,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(result.row_count, 1);
    assert!(session.schema_graph(Some("[]")).await.is_err());
    session.close().await;
    assert_eq!(
        session.schema_graph(None).await.unwrap_err().kind,
        ErrorKind::Connection
    );
    for name in [view, orders, users, empty, sampled] {
        db.collection::<Document>(&name).drop().await.unwrap();
    }
    db.drop().await.unwrap();
    client.shutdown().await;
}

#[tokio::test]
async fn real_mongodb_schema_collection_limit() {
    let Ok(uri) = std::env::var("AXDB_TEST_MONGO") else {
        eprintln!("AXDB_TEST_MONGO unset: collection limit integration test not run");
        return;
    };
    assert_eq!(
        uri,
        "mongodb://axdb:testmongo@127.0.0.1:57017/?authSource=admin"
    );
    let client = Client::with_uri_str(uri).await.unwrap();
    let name = format!("axdbt_schema_limit_{}", ObjectId::new().to_hex());
    let db = client.database(&name);
    for i in 0..101 {
        db.create_collection(format!("axdbt_{i}")).await.unwrap();
    }
    let cfg = ConnectConfig {
        fields: BTreeMap::from([
            ("uri".into(), "mongodb://127.0.0.1:57017".into()),
            ("database".into(), name),
            ("user".into(), "axdb".into()),
        ]),
        secret: Some("testmongo".into()),
        connect_timeout_ms: 3000,
    };
    let session = MongoDriver::new().connect(&cfg).await.unwrap();
    let error = session.schema_graph(None).await.unwrap_err();
    assert_eq!(error.kind, ErrorKind::Unsupported);
    assert!(error.message.contains("100 collections"));
    session.close().await;
    db.drop().await.unwrap();
    client.shutdown().await;
}
