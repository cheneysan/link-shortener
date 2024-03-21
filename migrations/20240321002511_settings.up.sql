create table if not exists settings
(
    id                       text default 'DEFAULT_SETTINGS' not null primary key,
    encrypted_global_api_key text                            not null
);

insert into settings (encrypted_global_api_key) values ('c0067d4af4e87f00dbac63b6156828237059172d1bbeac67427345d6a9fda484');
-- default password is password