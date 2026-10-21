-- Each connection's market orders, as `/characters/{id}/orders/` returns
-- them. The `market_orders` character source upserts the returned orders and
-- closes the connection's other open ones (filled, expired or cancelled);
-- closed rows stay for order history. `order_range` uses the same encoding
-- as market_order_observations: station -1, system 0, region 32767,
-- otherwise the jump count.
CREATE TABLE character_market_orders (
    connection_id uuid NOT NULL REFERENCES eve_connections(id) ON DELETE CASCADE,
    order_id bigint NOT NULL CHECK (order_id > 0),
    type_id bigint NOT NULL CHECK (type_id > 0),
    region_id bigint NOT NULL CHECK (region_id > 0),
    location_id bigint NOT NULL CHECK (location_id > 0),
    order_side text NOT NULL CHECK (order_side IN ('buy', 'sell')),
    is_corporation boolean NOT NULL,
    price numeric(24, 4) NOT NULL CHECK (price > 0),
    volume_total bigint NOT NULL CHECK (volume_total > 0),
    volume_remain bigint NOT NULL CHECK (volume_remain >= 0),
    min_volume bigint NOT NULL CHECK (min_volume > 0),
    order_range integer NOT NULL,
    issued_at timestamptz NOT NULL,
    duration_days integer NOT NULL CHECK (duration_days >= 0),
    escrow numeric(24, 4),
    first_seen_at timestamptz NOT NULL,
    last_seen_at timestamptz NOT NULL,
    closed_at timestamptz,
    PRIMARY KEY (connection_id, order_id)
);

CREATE INDEX character_market_orders_open_idx
    ON character_market_orders (connection_id) WHERE closed_at IS NULL;
