# The skf labs' shape on a release Isoloom has: a stage chain, a user, pip --user.
FROM alpine:3.19 AS skf-alpine
RUN apk update --no-cache && apk add python3 py3-pip bash
FROM skf-alpine
RUN addgroup -g 1000 app
RUN adduser -u 1000 -G app -D -h /home/app app
COPY ./ /home/app/CMD
WORKDIR /home/app/CMD
RUN chown -R app:app /home/app/CMD
USER app
RUN pip3 install --no-cache-dir --user --break-system-packages -r requirements.txt
ENTRYPOINT [ "python3", "./CMD.py" ]
