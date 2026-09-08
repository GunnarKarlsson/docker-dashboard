FROM alpine:3.20

COPY scripts /scripts
RUN chmod +x /scripts/*.sh

WORKDIR /data
CMD ["/scripts/api.sh"]
