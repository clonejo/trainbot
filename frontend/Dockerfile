ARG DOCKER_BASE_IMAGE
FROM ${DOCKER_BASE_IMAGE} AS build

WORKDIR /app
COPY package.json yarn.lock ./
RUN yarn install

COPY . .
RUN \
	#VITE_BASE_URL=http://localhost:5173/ \
	#VITE_DB_URL=http://localhost:5173/data/db.sqlite3 \
	#VITE_BLOBS_URL=http://localhost:5173/data/blobs \
	VITE_BASE_URL=https://trains.shakik.de/ \
	VITE_DB_URL=https://trains.shakik.de/data/db.sqlite3 \
	VITE_BLOBS_URL=https://trains.shakik.de/data/blobs \
	yarn run build

FROM scratch AS export
COPY --from=build /app/dist /
