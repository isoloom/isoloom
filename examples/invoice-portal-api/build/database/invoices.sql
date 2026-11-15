-- The portal's invoices: made-up rows. INV-1042's amount is set at start by the setup step.
CREATE TABLE IF NOT EXISTS invoices (
  reference VARCHAR(16) PRIMARY KEY,
  vendor    VARCHAR(64) NOT NULL,
  amount    DECIMAL(10,2) NOT NULL,
  status    VARCHAR(16) NOT NULL
);

INSERT IGNORE INTO invoices VALUES
  ('INV-1040', 'example-supplies', 410.00, 'approved'),
  ('INV-1041', 'example-supplies', 98.50, 'pending'),
  ('INV-1042', 'example-logistics', 0.00, 'pending');
