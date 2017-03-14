// Copyright © 2016, Canal TP and/or its affiliates. All rights reserved.
//
// This file is part of Navitia,
//     the software to build cool stuff with public transport.
//
// Hope you'll enjoy and contribute to this project,
//     powered by Canal TP (www.canaltp.fr).
// Help us simplify mobility and open public transport:
//     a non ending quest to the responsive locomotion way of traveling!
//
// LICENCE: This program is free software; you can redistribute it
// and/or modify it under the terms of the GNU Affero General Public
// License as published by the Free Software Foundation, either
// version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but
// WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU
// Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public
// License along with this program. If not, see
// <http://www.gnu.org/licenses/>.
//
// Stay tuned using
// twitter @navitia
// IRC #navitia on freenode
// https://groups.google.com/d/forum/navitia
// www.navitia.io
use rustless;
use serde;
use serde_json;
use rustless::server::{status, header};
use rustless::{Api, Nesting};
use valico::json_dsl;
use super::query;
use model::v1::*;
use model;
use params::{dataset_builder, paginate_builder, shape_builder, types_builder, coord_builder};

const DEFAULT_LIMIT: u64 = 10u64;
const DEFAULT_OFFSET: u64 = 0u64;


fn render<T>(mut client: rustless::Client,
             obj: T)
             -> Result<rustless::Client, rustless::ErrorResponse>
    where T: serde::Serialize
{
    client.set_json_content_type();
    client.set_header(header::AccessControlAllowOrigin::Any);
    client.text(serde_json::to_string(&obj).unwrap())
}


pub struct ApiEndPoint {
    pub es_cnx_string: String,
}


impl ApiEndPoint {
    pub fn root(&self) -> rustless::Api {
        Api::build(|api| {
            api.get("", |endpoint| {
                endpoint.handle(|client, _params| {
                    let desc = EndPoint { description: "autocomplete service".to_string() };
                    render(client, desc)
                })
            });

            api.error_formatter(|error, _media| {
                let err = if error.is::<rustless::errors::Validation>() {
                    let val_err = error.downcast::<rustless::errors::Validation>().unwrap();
                    // TODO better message, we shouldn't use {:?} but access the `path`
                    // and `detail` of all errrors in val_err.reason
                    CustomError {
                        short: "validation error".to_string(),
                        long: format!("invalid arguments {:?}", val_err.reason),
                    }
                } else {
                    CustomError {
                        short: "bad_request".to_string(),
                        long: format!("bad request, error: {}", error),
                    }
                };
                let mut resp = rustless::Response::from(status::StatusCode::BadRequest,
                                                        Box::new(serde_json::to_string(&err)
                                                            .unwrap()));
                resp.set_json_content_type();
                Some(resp)
            });
            api.mount(self.v1());
        })
    }

    fn v1(&self) -> rustless::Api {
        Api::build(|api| {
            api.mount(self.status());
            api.mount(self.autocomplete());
            api.mount(self.features());
        })
    }

    fn status(&self) -> rustless::Api {
        Api::build(|api| {
            api.get("status", |endpoint| {
                let cnx = self.es_cnx_string.clone();
                endpoint.handle(move |client, _params| {
                    let status = Status {
                        version: env!("CARGO_PKG_VERSION").to_string(),
                        es: cnx.to_string(),
                        status: "good".to_string(),
                    };
                    render(client, status)
                })
            });
        })
    }

    fn features(&self) -> rustless::Api {
        Api::build(|api| {
            api.get("features/:id", |endpoint| {
                endpoint.params(|params| {
                    params.opt_typed("id", json_dsl::string());
                    dataset_builder(params);
                });

                let cnx = self.es_cnx_string.clone();
                endpoint.handle(move |client, params| {
                    let id = params.find("id");
                    let pt_dataset = params.find("pt_dataset")
                        .and_then(|val| val.as_str());
                    let all_data =
                        params.find("_all_data").and_then(|val| val.as_bool()).unwrap_or(false);

                    let features = query::features(&pt_dataset, all_data, &cnx, &id);

                    let response = model::v1::AutocompleteResponse::from(features);
                    render(client, response)
                })
            });
        })
    }

    fn autocomplete(&self) -> rustless::Api {
        Api::build(|api| {
            api.post("autocomplete", |endpoint| {
                endpoint.params(|params| {
                    params.opt_typed("q", json_dsl::string());
                    dataset_builder(params);
                    paginate_builder(params);
                    shape_builder(params);
                    types_builder(params);
                });

                let cnx = self.es_cnx_string.clone();
                endpoint.handle(move |client, params| {
                    let q = params.find("q").and_then(|val| val.as_str()).unwrap_or("").to_string();
                    let pt_dataset = params.find("pt_dataset")
                        .and_then(|val| val.as_str());
                    let all_data =
                        params.find("_all_data").and_then(|val| val.as_bool()).unwrap_or(false);
                    let offset = params.find("offset")
                        .and_then(|val| val.as_u64())
                        .unwrap_or(DEFAULT_OFFSET);
                    let limit =
                        params.find("limit").and_then(|val| val.as_u64()).unwrap_or(DEFAULT_LIMIT);
                    let geometry = params.find_path(&["shape", "geometry"]).unwrap();
                    let coordinates =
                        geometry.find_path(&["coordinates"]).unwrap().as_array().unwrap();
                    let mut shape = Vec::new();
                    for ar in coordinates[0].as_array().unwrap() {
                        // (Lat, Lon)
                        shape.push((ar.as_array().unwrap()[1].as_f64().unwrap(),
                                    ar.as_array().unwrap()[0].as_f64().unwrap()));
                    }
                    let types = params.find("type")
                        .and_then(|val| val.as_array())
                        .map(|val| val.iter().map(|val| val.as_str().unwrap()).collect());
                    let model_autocomplete = query::autocomplete(&q,
                                                                 &pt_dataset,
                                                                 all_data,
                                                                 offset,
                                                                 limit,
                                                                 None,
                                                                 &cnx,
                                                                 Some(shape),
                                                                 types);

                    let response = model::v1::AutocompleteResponse::from(model_autocomplete);
                    render(client, response)
                })
            });
            api.get("autocomplete", |endpoint| {
                endpoint.params(|params| {
                    params.opt_typed("q", json_dsl::string());
                    dataset_builder(params);
                    paginate_builder(params);
                    coord_builder(params);
                    types_builder(params);
                });
                let cnx = self.es_cnx_string.clone();
                endpoint.handle(move |client, params| {
                    let q = params.find("q").and_then(|val| val.as_str()).unwrap_or("").to_string();
                    let pt_dataset = params.find("pt_dataset")
                        .and_then(|val| val.as_str());
                    let all_data =
                        params.find("_all_data").and_then(|val| val.as_bool()).unwrap_or(false);
                    let offset = params.find("offset")
                        .and_then(|val| val.as_u64())
                        .unwrap_or(DEFAULT_OFFSET);
                    let limit =
                        params.find("limit").and_then(|val| val.as_u64()).unwrap_or(DEFAULT_LIMIT);
                    let lon = params.find("lon").and_then(|p| p.as_f64());
                    let lat = params.find("lat").and_then(|p| p.as_f64());
                    // we have already checked that if there is a lon, lat
                    // is not None so we can unwrap
                    let coord = lon.and_then(|lon| {
                        Some(model::Coord {
                            lon: lon,
                            lat: lat.unwrap(),
                        })
                    });

                    let types = params.find("type")
                        .and_then(|val| val.as_array())
                        .map(|val| val.iter().map(|val| val.as_str().unwrap()).collect());

                    let model_autocomplete = query::autocomplete(&q,
                                                                 &pt_dataset,
                                                                 all_data,
                                                                 offset,
                                                                 limit,
                                                                 coord,
                                                                 &cnx,
                                                                 None,
                                                                 types);

                    let response = model::v1::AutocompleteResponse::from(model_autocomplete);
                    render(client, response)
                })
            });
        })
    }
}
