-- SPDX-License-Identifier: Apache-2.0
-- Written by Isoloom from the catalogue's mysql entry: the application user's privileges of its
-- `privileges` setting, in place of all of them. Run once at the first start, after the user is
-- made.
REVOKE ALL PRIVILEGES ON `{{ with.database }}`.* FROM '{{ with.user }}'@'%';
GRANT {{ with.privileges }} ON `{{ with.database }}`.* TO '{{ with.user }}'@'%';
FLUSH PRIVILEGES;
