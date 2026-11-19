-- PostgreSQL 19 forms, including aliases kept by the upstream grammar.
repack (verbose true, "Exact Option" 2.75) app."Work.Queue" ("select", "Ä") using index "index";
repack using index;
repack app.t;
cluster verbose "I" on app.t;
cluster;
wait for lsn '0/16B6C50' with (timeout -20, mode on);
checkpoint (mode fast);
listen "someone""else";
notify "select", E'it''s\\ready';
unlisten *;
unlisten wait;
load 'f''o';
discard temp;
close all;
deallocate prepare "ALL";
deallocate all
