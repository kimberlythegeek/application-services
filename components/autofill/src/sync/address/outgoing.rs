/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use crate::db::models::address::InternalAddress;
use crate::db::schema::ADDRESS_COMMON_COLS;
use crate::error::*;
use crate::sync::common::*;
use crate::sync::{
    OutgoingChangeset, Payload, ProcessOutgoingRecordImpl, ServerTimestamp, SyncRecord,
};
use rusqlite::{Row, Transaction};
use sync_guid::Guid as SyncGuid;

pub(super) struct OutgoingAddressesImpl {}

impl ProcessOutgoingRecordImpl for OutgoingAddressesImpl {
    type Record = InternalAddress;

    /// Gets the local records that have unsynced changes or don't have corresponding mirror
    /// records and upserts them to the mirror table
    fn fetch_outgoing_records(
        &self,
        tx: &Transaction<'_>,
        collection_name: String,
        timestamp: ServerTimestamp,
    ) -> anyhow::Result<OutgoingChangeset> {
        let mut outgoing = OutgoingChangeset::new(collection_name, timestamp);

        let data_sql = format!(
            "SELECT
                {common_cols},
                sync_change_counter
            FROM addresses_data
            WHERE sync_change_counter > 0
                OR guid NOT IN (
                    SELECT m.guid
                    FROM addresses_mirror m
                )",
            common_cols = ADDRESS_COMMON_COLS,
        );
        let payload_from_data_row: &dyn Fn(&Row<'_>) -> Result<Payload> =
            &|row| Ok(InternalAddress::from_row(row)?.to_payload().unwrap());

        let tombstones_sql = "SELECT guid FROM addresses_tombstones";

        // save outgoing records to the mirror table
        let staging_records = common_get_outgoing_staging_records(
            &tx,
            &data_sql,
            &tombstones_sql,
            payload_from_data_row,
        )?;
        common_save_outgoing_records(&tx, "addresses_sync_outgoing_staging", staging_records)?;

        // return outgoing changes
        let outgoing_records: Vec<(Payload, i64)> =
            common_get_outgoing_records(&tx, &data_sql, &tombstones_sql, payload_from_data_row)?;

        outgoing.changes = outgoing_records
            .iter()
            .map(|(payload, _)| payload.clone())
            .collect::<Vec<Payload>>();
        Ok(outgoing)
    }

    fn push_synced_items(
        &self,
        tx: &Transaction<'_>,
        records_synced: Vec<SyncGuid>,
    ) -> anyhow::Result<()> {
        let outgoing_table_name = "addresses_sync_outgoing_staging";
        let data_table_name = "addresses_data";
        common_reset_sync_change_counter(
            &tx,
            data_table_name,
            outgoing_table_name,
            records_synced,
        )?;
        common_push_outgoing_records(&tx, "addresses_mirror", outgoing_table_name)?;
        Ok(())
    }
}
