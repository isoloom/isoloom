FROM python:3.12 AS build
RUN pip wheel -w /w flask
FROM alpine:3.19
COPY --from=build /w /w
