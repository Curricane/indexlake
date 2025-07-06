# IndexLake Design - 00: Project Overview

## 1. Introduction

IndexLake is a storage engine designed to function as a specialized "lakehouse for indices". It provides a unified, transactional, and extensible framework for managing and querying large-scale datasets alongside their secondary indices.

The core value proposition is to treat various types of indices (spatial, full-text, vector, etc.) as first-class citizens, co-located with the primary data, enabling accelerated queries that would otherwise require full data scans.

## 2. Core Philosophy & Design Principles

The architecture of IndexLake is guided by the following principles, reflecting best practices from modern database and data lakehouse design:

- **Decoupling and Abstraction**: The system is built upon a set of clearly defined `trait`s (interfaces) that decouple different layers of the system. This is the most critical principle, enabling modularity and extensibility.
  - The **Catalog** (metadata store), **Storage** (physical data placement), and **Index** (acceleration structures) are all behind abstraction layers, allowing for different implementations to be plugged in.

- **Separation of Metadata and Data**: The system maintains a clear distinction between the logical metadata (managed by the Catalog) and the physical data/index files (managed by the Storage layer). This allows for flexible data placement (local FS, S3) and robust metadata management (Postgres, SQLite).

- **Atomicity and Transactionality**: All metadata operations are transactional. This is primarily managed by the `Catalog` and its `Transaction` trait, ensuring that complex operations like table creation (which involves multiple metadata updates) are atomic. The use of RAII (via `Drop` trait) on transaction objects ensures automatic rollback on failure, guaranteeing system consistency.

- **Extensibility through Pluggable Frameworks**: The indexing layer is designed as a fully pluggable framework. New index types (e.g., for full-text search like BM25 or vector search like HNSW) can be added by simply implementing the `Index` and `IndexBuilder` traits, without requiring changes to the core query engine.

- **Hybrid Data Storage Model**: IndexLake employs a sophisticated data layout strategy that balances performance for small and large datasets.
  - **Inline Data**: Small tables or the most recent data insertions are stored "inline" directly within the Catalog's database tables for extremely low-latency access.
  - **External Data Files**: As data grows, a background "dump" process transparently moves data from inline storage to optimized, columnar external files (Apache Parquet) in the Storage layer.
  - This is managed by the `row_metadata` table, which acts as a global address book, tracking the physical location of every row.

- **Leveraging Apache Arrow**: The project uses Apache Arrow as its in-memory data format. This provides a standardized, highly efficient, and language-agnostic columnar memory format, enabling high-performance analytics and data manipulation.

## 3. High-Level Goal

The ultimate goal of IndexLake is to provide a simple, robust, and high-performance foundation for building data-intensive applications that require fast, indexed queries over large and diverse datasets. It aims to combine the scalability and cost-effectiveness of a data lake with the performance and transactional guarantees of a traditional database, with a special focus on making secondary indices a core, manageable part of the architecture.
