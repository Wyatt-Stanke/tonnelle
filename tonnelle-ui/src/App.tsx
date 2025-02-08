import { LineChart, Line, XAxis, YAxis, Tooltip } from "recharts";
import { useState, useEffect } from "react";

const GLOBAL_IP = "129.80.117.126";

function App() {
	const [data, setData] = useState([]);
	const [error, setError] = useState(null);
	const [isFetching, setIsFetching] = useState(false);

	useEffect(() => {
		const handle = setInterval(() => {
			if (!isFetching) {
				setIsFetching(true);
				fetch(`http://${GLOBAL_IP}/status`)
					.then((res) => res.json())
					.then((jsonData) => {
						setData((prevData) => {
							const newData = [
								...prevData,
								{
									name: new Date().toLocaleTimeString(),
									value: jsonData.warm_sockets,
								},
							];
							return newData.slice(-20); // Keep only the latest 20 samples
						});
						setIsFetching(false);
					})
					.catch((err) => {
						setError(err);
						setIsFetching(false);
					});
			}
		}, 1000);
		return () => clearInterval(handle);
	}, [isFetching]);

	if (error) return <div>Failed to load</div>;
	if (!data.length) return <div>Loading...</div>;

	return (
		<div className="p-4">
			<h1 className="text-xl font-bold">Proxy Status</h1>
			<LineChart width={500} height={300} data={data}>
				<XAxis dataKey="name" />
				<YAxis />
				<Tooltip />
				<Line type="monotone" dataKey="value" stroke="#8884d8" />
			</LineChart>
		</div>
	);
}

export default App;
