# for test can run in wsl2 as volume not work in wsl2 sometimes
FROM postgres:latest
COPY init_catalog.sql /docker-entrypoint-initdb.d/init.sql
