# distance-db-populator

Populate the Distance Database with data from Steam.

This software is designed to be run with Docker. The following environment variables should be set:

- `DATABASE_URL`: URL of the Postgres DB that will be updated
- `STEAM_WEB_API_KEY`: Steam Web API key; you can get one [here](https://steamcommunity.com/dev/apikey).
- `GRPC_SERVER_ADDRESS`: Address of a [DistanceSteamDataServer](https://github.com/Seeker14491/DistanceSteamDataServer)
- `MIN_MINUTES_BETWEEN_UPDATES`: Wait at least this many minutes between running the populator

Optionally, the variable `HEALTHCHECKS_URL` can be set to a [healthchecks.io](https://healthchecks.io/) ping url.

## Misc.

Dumping the database:

```
pg_dump -s -d distance -f create_db.sql
```
