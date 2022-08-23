# distance-db-populator

Populate the Distance Database with data from Steam.

This software is designed to be run with Docker. The following environment variables should be set:

- `STEAM_USERNAME` and `STEAM_PASSWORD`: Credentials to authenticate to the Steam servers. The account must own Distance, and Steam Guard must be disabled.
- `DATABASE_URL`: URL of the Postgres DB that will be updated
- `GRPC_SERVER_ADDRESS`: Address of a [DistanceSteamDataServer](https://github.com/Seeker14491/DistanceSteamDataServer)
- `MIN_MINUTES_BETWEEN_UPDATES`: Wait at least this many minutes between running the populator

Optionally, the variable `HEALTHCHECKS_URL` can be set to a [healthchecks.io](https://healthchecks.io/) ping url.

It's recommended to persist the container path `/root/.steam` so the Steam client doesn't spend time updating needlessly.

## Misc.

Dumping the database:

```
pg_dump -s -d distance -f create_db.sql
```